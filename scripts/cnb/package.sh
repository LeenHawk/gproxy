#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../reproducible-env.sh"
source scripts/cnb/env.sh
: "${RELEASE_RUN_ID:?}"
: "${TARGET_TRIPLE:?}"
mkdir -p dist/release

download_console() {
  mkdir -p console/dist
  python3 scripts/cnb/api.py download "$RELEASE_RUN_ID-console.tar.gz" "$RUNNER_TEMP/console.tar.gz"
  tar -xzf "$RUNNER_TEMP/console.tar.gz" -C console/dist
  node console/scripts/sync-to-embed.mjs
}

install_upx() {
  if [ "${UPX_ENABLED:-false}" != true ]; then return; fi
  if [ "$TARGET_TRIPLE" = aarch64-pc-windows-msvc ]; then
    # Same upstream UPX revision as the native Windows release job, built
    # for the Linux host. The compressed output is still Windows ARM64.
    local source_dir="${XDG_CACHE_HOME:-$HOME/.cache}/gproxy/windows-arm64-upx-b39171b1"
    mkdir -p "$(dirname "$source_dir")"
    if [ ! -x "$source_dir/build/upx" ]; then
      git init "$source_dir"
      git -C "$source_dir" config remote.origin.url https://github.com/upx/upx.git
      git -C "$source_dir" fetch --depth 1 origin b39171b1d215cd6427ff0f4939f0cadc316d17f2
      git -C "$source_dir" checkout --detach FETCH_HEAD
      git -C "$source_dir" submodule update --init --recursive --depth 1
      cmake -S "$source_dir" -B "$source_dir/build" -DCMAKE_BUILD_TYPE=Release -DUPX_CONFIG_DISABLE_WERROR=ON
      cmake --build "$source_dir/build" --parallel 4
    fi
    export PATH="$source_dir/build:$PATH"
  else
    scripts/install-upx.sh
    cnb_tool_paths
  fi
}

cross_environment() {
  case "$TARGET_OS" in
    windows) export RUSTFLAGS='-C target-feature=+crt-static' ;;
    macos)
      export SDKROOT=/opt/MacOSX26.1.sdk
      export MACOSX_DEPLOYMENT_TARGET=10.13
      [[ "$TARGET_TRIPLE" != aarch64-* ]] || export MACOSX_DEPLOYMENT_TARGET=11.0
      ;;

  esac
}

upload_bundle() {
  local name="$1"
  local entries=(release)
  [ ! -d dist/container ] || entries+=(container)
  tar -czf "$RUNNER_TEMP/$RELEASE_RUN_ID-$name.tar.gz" -C dist "${entries[@]}"
  python3 scripts/cnb/api.py upload "$RUNNER_TEMP/$RELEASE_RUN_ID-$name.tar.gz"
}

case "${1:?prepare|cli|application|upload|edge}" in
  prepare)
    download_console
    [ "${BUILDER:-}" = cargo-alpine ] || rustup target add "$TARGET_TRIPLE"
    ;;
  cli)
    cross_environment
    case "$TARGET_OS" in
      linux)
        if [ "$BUILDER" = cargo-alpine ]; then scripts/build-musl.sh "$TARGET_TRIPLE"
        else python3 scripts/reproducible-run.py cargo build --locked --release --bin gproxy --target "$TARGET_TRIPLE"; fi
        mkdir -p "dist/container/$TARGET_TRIPLE"
        cp "target/$TARGET_TRIPLE/release/gproxy" "dist/container/$TARGET_TRIPLE/gproxy"
        ;;
      windows)
        version_numbers="${GPROXY_BUILD_VERSION%%-*}"
        sed -e "s/__VERSION__/$GPROXY_BUILD_VERSION/g" -e "s/__NUMERIC_VERSION__/${version_numbers//./,},0/g" \
          scripts/installers/windows/gproxy.rc.in > "$RUNNER_TEMP/gproxy.rc"
        llvm-rc /fo "$RUNNER_TEMP/gproxy.res" "$RUNNER_TEMP/gproxy.rc"
        cargo xwin rustc --locked --release -p gproxy --bin gproxy --target "$TARGET_TRIPLE" -- -C "link-arg=$RUNNER_TEMP/gproxy.res"
        export BUILDER=cargo-xwin
        ;;
      macos)
        python3 scripts/reproducible-run.py cargo zigbuild --locked --release --bin gproxy --target "$TARGET_TRIPLE"
        rcodesign sign "target/$TARGET_TRIPLE/release/gproxy"
        python3 scripts/cnb/verify-macho.py "target/$TARGET_TRIPLE/release/gproxy"
        export BUILDER=cargo-zigbuild
        ;;
      android)
        bash scripts/build-termux.sh "$TARGET_TRIPLE"
        ;;
    esac
    binary="target/$TARGET_TRIPLE/release/gproxy"
    [ "$TARGET_OS" != windows ] || binary="$binary.exe"
    install_upx
    if [ "$UPX_ENABLED" = true ]; then
      upx --best --lzma "$binary"
      upx --test "$binary"
    fi
    if [ "$TARGET_OS" = linux ]; then "$binary" --version; "$binary" --help >/dev/null; fi
    if [ "$TARGET_TRIPLE" = x86_64-pc-windows-msvc ]; then
      WINEDEBUG=-all wine "$binary" --version
      WINEDEBUG=-all wine "$binary" --help >/dev/null
    fi
    if [ "$TARGET_OS" = windows ]; then
      work="$(mktemp -d)"
      cp "$binary" README.md LICENSE "$work/"
      python3 scripts/reproducible-archive.py --root "$work" --output "dist/release/$ARTIFACT_NAME.zip"
      rm -rf "$work"
      (cd dist/release && sha256sum "$ARTIFACT_NAME.zip" > "$ARTIFACT_NAME.zip.sha256")
    else
      scripts/package-native-release.sh
    fi
    scripts/build-provenance.sh
    ;;
  application)
    if [ "$TARGET_OS" = android ]; then export GPROXY_INSTALLATION_KIND=android-apk; fi
    test -n "${APPLICATION_ARTIFACT:?}"
    cross_environment
    export ARTIFACT_NAME="$APPLICATION_ARTIFACT" BUILDER=tauri
    pnpm --dir crates/gproxy-host-tauri install --frozen-lockfile
    install_upx
    case "$TARGET_OS" in
      linux) scripts/package-tauri-release.sh ;;
      android)
        scripts/install-android-upx.sh
        cnb_tool_paths
        scripts/package-tauri-release.sh
        ;;
      windows | macos)
        config="$(jq -cn --arg version "$GPROXY_BUILD_VERSION" '{version:$version,bundle:{active:true},build:{windows:{staticVCRuntime:false}}}')"
        runner=cargo-xwin
        [ "$TARGET_OS" != macos ] || runner=cargo-zigbuild
        (
          cd crates/gproxy-host-tauri
          # The Android entry-point DLL is unused on desktop. Build only rlib.
          backup="$(mktemp)"
          cp Cargo.toml "$backup"
          trap 'cp "$backup" Cargo.toml; rm -f "$backup"' EXIT
          sed -i 's/crate-type = \["lib", "cdylib"\]/crate-type = ["lib"]/' Cargo.toml
          pnpm exec tauri build --ci --runner "$runner" --target "$TARGET_TRIPLE" --no-bundle --config "$config" -- --locked
        )
        if [ "$TARGET_OS" = windows ]; then
          binary="target/$TARGET_TRIPLE/release/gproxy-desktop.exe"
          upx --best --lzma "$binary"
          upx --test "$binary"
          (cd crates/gproxy-host-tauri && pnpm exec tauri bundle --target "$TARGET_TRIPLE" --bundles nsis --config "$config" --no-binary-patching)
          files=("target/$TARGET_TRIPLE/release/bundle/nsis/"*.exe)
          test "${#files[@]}" -eq 1 && test -f "${files[0]}"
          cp "${files[0]}" "dist/release/$ARTIFACT_NAME.exe"
        else
          python3 scripts/cnb/macos-application.py
        fi
        (cd dist/release && for file in "$ARTIFACT_NAME".*; do sha256sum "$file" > "$file.sha256"; done)
        ;;
    esac
    scripts/build-provenance.sh
    ;;
  upload) upload_bundle "$TARGET_TRIPLE" ;;
  edge)
    download_console
    cargo install worker-build --version 0.8.7
    scripts/package-edge-release.sh
    scripts/build-provenance.sh
    mkdir -p "$RUNNER_TEMP/edge"
    unzip -q dist/release/gproxy-edge-cloudflare.zip -d "$RUNNER_TEMP/edge"
    pnpm --dir "$RUNNER_TEMP/edge/cloudflare" install --no-frozen-lockfile --ignore-scripts
    pnpm --dir "$RUNNER_TEMP/edge/cloudflare" check
    upload_bundle edge
    ;;
  *) exit 2 ;;
esac
