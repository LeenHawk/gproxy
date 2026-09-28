#!/usr/bin/env bash
set -euo pipefail
source scripts/gitlab/env.sh
export RUSTFLAGS='-C target-feature=+crt-static'
export WINEDEBUG=-all
export WINEPREFIX="$RUNNER_TEMP/wine"
mkdir -p dist/release
if [ "$TARGET_TRIPLE" = aarch64-pc-windows-msvc ]; then
  export PATH="/opt/upx-arm64/bin:$PATH"
else
  scripts/install-upx.sh
  release_tool_paths
fi

pack_executable() {
  if [ "$TARGET_TRIPLE" = aarch64-pc-windows-msvc ]; then
    upx --fast --nrv2e "$1"
  else
    upx --best --lzma "$1"
  fi
  upx --test "$1"
}

case "${1:?cli|application}" in
  cli)
    version_numbers="${GPROXY_BUILD_VERSION%%-*}"
    sed -e "s/__VERSION__/$GPROXY_BUILD_VERSION/g" -e "s/__NUMERIC_VERSION__/${version_numbers//./,},0/g" \
      scripts/installers/windows/gproxy.rc.in > "$RUNNER_TEMP/gproxy.rc"
    llvm-rc /fo "$RUNNER_TEMP/gproxy.res" "$RUNNER_TEMP/gproxy.rc"
    cargo xwin rustc --locked --release -p gproxy --bin gproxy --target "$TARGET_TRIPLE" -- -C "link-arg=$RUNNER_TEMP/gproxy.res"
    export BUILDER=cargo-xwin
    binary="target/$TARGET_TRIPLE/release/gproxy.exe"
    mkdir -p "dist/windows/$TARGET_TRIPLE"
    cp "$binary" "dist/windows/$TARGET_TRIPLE/gproxy-unpacked.exe"
    if [ "$TARGET_TRIPLE" = x86_64-pc-windows-msvc ]; then
      wine "$binary" --version
      wine "$binary" --help >/dev/null
    fi
    pack_executable "$binary"
    if [ "$TARGET_TRIPLE" = x86_64-pc-windows-msvc ]; then
      if wine "$binary" --version && wine "$binary" --help >/dev/null; then
        echo 'Packed CLI also starts under Wine.'
      else
        echo 'Packed CLI did not start under Wine; native Windows validation is required before publication.'
      fi
    fi
    cp "$binary" "dist/windows/$TARGET_TRIPLE/gproxy.exe"
    work="$(mktemp -d)"
    trap 'rm -rf "$work"' EXIT
    cp "$binary" README.md LICENSE "$work/"
    (cd "$work" && zip -9 -qr "$OLDPWD/dist/release/$ARTIFACT_NAME.zip" .)
    (cd dist/release && sha256sum "$ARTIFACT_NAME.zip" > "$ARTIFACT_NAME.zip.sha256")
    ;;
  application)
    export ARTIFACT_NAME="$APPLICATION_ARTIFACT" BUILDER=tauri-xwin
    pnpm --dir crates/gproxy-host-tauri install --frozen-lockfile
    # cargo-xwin supplies the static CRT flags. Avoid Tauri supplying a second
    # conflicting set, and omit the Android-only DLL from this desktop build.
    config="$(jq -cn --arg version "$GPROXY_BUILD_VERSION" '{version:$version,bundle:{active:true},build:{windows:{staticVCRuntime:false}}}')"
    (
      cd crates/gproxy-host-tauri
      backup="$(mktemp)"
      cp Cargo.toml "$backup"
      trap 'cp "$backup" Cargo.toml; rm -f "$backup"' EXIT
      sed -i 's/crate-type = \["lib", "cdylib"\]/crate-type = ["lib"]/' Cargo.toml
      pnpm exec tauri build --ci --runner cargo-xwin --target "$TARGET_TRIPLE" --no-bundle --config "$config" -- --locked
    )
    binary="target/$TARGET_TRIPLE/release/gproxy-desktop.exe"
    pack_executable "$binary"
    mkdir -p "dist/windows/$TARGET_TRIPLE"
    cp "$binary" "dist/windows/$TARGET_TRIPLE/"
    loader="target/$TARGET_TRIPLE/release/WebView2Loader.dll"
    if [ -f "$loader" ]; then cp "$loader" "dist/windows/$TARGET_TRIPLE/"; fi
    ;;
  *) exit 2 ;;
esac
scripts/build-provenance.sh
