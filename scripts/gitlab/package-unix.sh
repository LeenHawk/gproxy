#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../reproducible-env.sh"
source scripts/gitlab/env.sh
mkdir -p dist/release
case "${1:?cli|application}" in
  cli)
    echo "Building CLI / server: $ARTIFACT_NAME"
    case "$BUILDER" in
      cargo) python3 scripts/reproducible-run.py cargo build --locked --release --bin gproxy --target "$TARGET_TRIPLE" ;;
      cargo-alpine) scripts/build-musl.sh "$TARGET_TRIPLE" ;;
      termux) bash scripts/build-termux.sh "$TARGET_TRIPLE" ;;
    esac
    binary="target/$TARGET_TRIPLE/release/gproxy"
    if [ "$TARGET_OS" = macos ]; then codesign --force --sign - "$binary"; codesign --verify --strict "$binary"; fi
    if [ "$UPX_ENABLED" = true ]; then
      scripts/install-upx.sh
      release_tool_paths
      upx --best --lzma "$binary"
      upx --test "$binary"
    fi
    if [ "$TARGET_OS" != android ]; then "$binary" --version; "$binary" --help >/dev/null; fi
    if [ "$TARGET_OS" = linux ]; then
      mkdir -p "dist/container/$TARGET_TRIPLE"
      cp "$binary" "dist/container/$TARGET_TRIPLE/gproxy"
    fi
    scripts/package-native-release.sh
    scripts/build-provenance.sh
    ;;
  application)
    if [ "$TARGET_OS" = android ]; then export GPROXY_INSTALLATION_KIND=android-apk; fi
    [ -n "$APPLICATION_ARTIFACT" ] || exit 0
    export ARTIFACT_NAME="$APPLICATION_ARTIFACT" BUILDER=tauri
    echo "Building Application: $ARTIFACT_NAME"
    pnpm --dir crates/gproxy-host-tauri install --frozen-lockfile
    if [ "$UPX_ENABLED" = true ]; then scripts/install-upx.sh; release_tool_paths; fi
    if [ "$TARGET_OS" = android ]; then scripts/install-android-upx.sh; release_tool_paths; fi
    scripts/package-tauri-release.sh
    scripts/build-provenance.sh
    ;;
  *) exit 2 ;;
esac
