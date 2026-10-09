#!/usr/bin/env bash
# Exercise the production build/package scripts without publishing or signing.
set -euo pipefail
export TARGET_TRIPLE="${1:?target}"
kind="${2:?cli|headless|application|edge|serverless}"
export CI_PROJECT_DIR="$PWD" CI_COMMIT_SHA="$GPROXY_BUILD_HASH"
export GITHUB_SHA="$GPROXY_BUILD_HASH" GITHUB_REF_TYPE=tag
export GITHUB_REF_NAME="v$(bash scripts/release-metadata.sh version)"
export CI_COMMIT_TAG="$GITHUB_REF_NAME"
requested_target="$TARGET_TRIPLE"
if [[ "$kind" = edge || "$kind" = serverless ]]; then export TARGET_TRIPLE=; fi
source scripts/gitlab/env.sh
export TARGET_TRIPLE="$requested_target"
if [ "$kind" = edge ]; then
  export TARGET_OS=wasm UPX_ENABLED=false ARTIFACT_NAME=gproxy-edge
elif [ "$kind" = serverless ]; then
  export TARGET_OS=linux UPX_ENABLED=true ARTIFACT_NAME="gproxy-serverless-linux-${TARGET_TRIPLE%%-*}-musl"
fi
source scripts/reproducible-env.sh
export GPROXY_UNSIGNED_BUILD=1
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
if [ "$TARGET_OS" = android ] && [[ "$kind" = application || "$kind" = store-* ]]; then
  # The F-Droid recipe uses this same NDK path (and the same pinned NDK).
  ndk_link="$(dirname "$CARGO_HOME")/android-ndk"
  ln -sfn "$ANDROID_NDK_HOME" "$ndk_link"
  export ANDROID_NDK_HOME="$ndk_link" ANDROID_NDK_ROOT="$ndk_link" NDK_HOME="$ndk_link"
fi

if [ "$kind" != headless ]; then
  pnpm --dir console install --frozen-lockfile
  pnpm --dir console build
fi
if [[ "$kind" = application || "$kind" = store-* ]]; then
  pnpm --dir crates/gproxy-host-tauri install --frozen-lockfile
fi

if [[ "$TARGET_TRIPLE" == riscv64gc-* ]] && ! { [ "$kind" = application ] && [[ "$TARGET_TRIPLE" == *-musl ]]; }; then
  exec bash scripts/gitlab/package-cross.sh "$kind"
fi
if [ "$TARGET_OS" = ohos ]; then
  if [ "$kind" = application ]; then python3 scripts/ohos/prepare-tauri.py; fi
  exec bash scripts/ohos/build.sh "$kind"
fi
if [ "$TARGET_OS" = windows ]; then
  export RUSTFLAGS='-C target-feature=+crt-static'
  if [ "$TARGET_TRIPLE" = aarch64-pc-windows-msvc ]; then
    export CMAKE_TOOLCHAIN_FILE_aarch64_pc_windows_msvc="$(cygpath -m "$PWD/scripts/cmake/windows-arm64.cmake")"
    pwsh -NoProfile -File scripts/install-windows-arm64-upx.ps1
  else
    bash scripts/install-upx.sh
  fi
elif [ "$UPX_ENABLED" = true ]; then
  bash scripts/install-upx.sh
fi
release_tool_paths
if [ "$TARGET_OS" = windows ] && [ "$UPX_ENABLED" = true ]; then upx --version; fi

case "$kind" in
  store-fdroid | store-google-play | store-appgallery)
    bash scripts/install-android-upx.sh
    release_tool_paths
    bash scripts/mobile/build-android.sh "${kind#store-}" universal
    mkdir -p dist/release
    cp dist/mobile/"${kind#store-}"/universal/* dist/release/
    ;;
  edge) bash scripts/package-edge-release.sh ;;
  serverless)
    bash scripts/build-serverless.sh "$TARGET_TRIPLE"
    bash scripts/package-serverless-release.sh
    ;;
  application)
    export ARTIFACT_NAME="$APPLICATION_ARTIFACT" APPLE_SIGNING_IDENTITY=-
    if [ "$TARGET_OS" = android ]; then
      export GPROXY_INSTALLATION_KIND=android-apk
      bash scripts/install-android-upx.sh
      release_tool_paths
    fi
    bash scripts/package-tauri-release.sh
    ;;
  cli | headless)
    features=(--locked)
    if [ "$kind" = headless ]; then
      export ARTIFACT_NAME="$(jq -r --arg target "$TARGET_TRIPLE" '.include[] | select(.target==$target) | .headless_artifact' scripts/release-targets.json)"
      export GPROXY_HEADLESS=true PACKAGE_INSTALLERS=false
      features=(--locked --no-default-features --features channels,memory,fs,bundled-vocabulary)
    fi
    case "$BUILDER" in
      cargo-alpine) bash scripts/build-musl.sh "$TARGET_TRIPLE" ;;
      termux) bash scripts/build-termux.sh "$TARGET_TRIPLE" ;;
      cargo)
        if [ "$TARGET_OS" = windows ]; then
          options=()
          [ "$kind" != headless ] || options=(-Headless)
          pwsh -NoProfile -File scripts/build-windows-release.ps1 -Target "$TARGET_TRIPLE" "${options[@]}"
        else
          python3 scripts/reproducible-run.py cargo build --release -p gproxy --bin gproxy --target "$TARGET_TRIPLE" "${features[@]}"
        fi
        ;;
      *) echo "Unsupported reproducibility builder: $BUILDER" >&2; exit 1 ;;
    esac
    binary="target/$TARGET_TRIPLE/release/gproxy"
    [ "$TARGET_OS" != windows ] || binary+=.exe
    if [ "$TARGET_OS" = linux ] && [ "$kind" = cli ]; then
      mkdir -p "dist/container/$TARGET_TRIPLE"
      cp "$binary" "dist/container/$TARGET_TRIPLE/gproxy"
    fi
    if [ "$UPX_ENABLED" = true ]; then
      upx --best --lzma "$binary"
      upx --test "$binary"
    fi
    if [ "$TARGET_OS" = macos ]; then codesign --force --sign - --timestamp=none "$binary"; fi
    if [ "$TARGET_OS" = windows ]; then
      pwsh -NoProfile -File scripts/package-native-release.ps1
      pwsh -NoProfile -File scripts/package-windows-msix.ps1 -Target "$TARGET_TRIPLE" -Artifact "$ARTIFACT_NAME" -Version "$GPROXY_BUILD_VERSION" -Mode "$kind" -OutputDir dist/release
    else
      bash scripts/package-native-release.sh
    fi
    ;;
  *) echo "Unsupported reproducibility product: $kind" >&2; exit 1 ;;
esac
