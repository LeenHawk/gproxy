#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../reproducible-env.sh"
source scripts/gitlab/env.sh
mode="${1:?cli|headless|application}"
export GOCACHE="$CI_PROJECT_DIR/.cache/go-build" GOMODCACHE="$CI_PROJECT_DIR/.cache/go-mod"
mkdir -p "$GOCACHE" "$GOMODCACHE" dist/release
case "$TARGET_OS" in
  ohos)
    export OHOS_TOOLCHAIN_IMAGE="${CI_JOB_IMAGE:-}"
    if [ "$mode" = application ]; then python3 scripts/ohos/prepare-tauri.py; fi
    exec bash scripts/ohos/build.sh "$mode"
    ;;
  windows) exec bash scripts/gitlab/package-windows-cross.sh "$mode" ;;
  android) exec bash scripts/gitlab/package-unix.sh "$mode" ;;
  macos)
    export SDKROOT=/opt/MacOSX26.1.sdk
    export MACOSX_DEPLOYMENT_TARGET=10.13
    [[ "$TARGET_TRIPLE" != aarch64-* ]] || export MACOSX_DEPLOYMENT_TARGET=11.0
    ;;
  linux)
    if [ "$TARGET_TRIPLE" = aarch64-unknown-linux-gnu ]; then
      export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
      export CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++
      export AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ar
      export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
      export CMAKE_TOOLCHAIN_FILE_aarch64_unknown_linux_gnu="$CI_PROJECT_DIR/scripts/cmake/linux-aarch64.cmake"
      export PKG_CONFIG_ALLOW_CROSS=1
      export PKG_CONFIG_LIBDIR=/usr/lib/aarch64-linux-gnu/pkgconfig:/usr/share/pkgconfig
      export BINDGEN_EXTRA_CLANG_ARGS_aarch64_unknown_linux_gnu='--target=aarch64-linux-gnu --sysroot=/'
    elif [ "$TARGET_TRIPLE" = riscv64gc-unknown-linux-gnu ]; then
      export CC_riscv64gc_unknown_linux_gnu=riscv64-linux-gnu-gcc
      export CXX_riscv64gc_unknown_linux_gnu=riscv64-linux-gnu-g++
      export AR_riscv64gc_unknown_linux_gnu=riscv64-linux-gnu-ar
      export CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_GNU_LINKER=riscv64-linux-gnu-gcc
      export CMAKE_TOOLCHAIN_FILE_riscv64gc_unknown_linux_gnu="$CI_PROJECT_DIR/scripts/cmake/linux-riscv64.cmake"
      export PKG_CONFIG_ALLOW_CROSS=1
      export PKG_CONFIG_LIBDIR=/usr/lib/riscv64-linux-gnu/pkgconfig:/usr/share/pkgconfig
      export BINDGEN_EXTRA_CLANG_ARGS_riscv64gc_unknown_linux_gnu='--target=riscv64-linux-gnu --sysroot=/'
    fi
    ;;
  *) exit 2 ;;
esac
if [ "$UPX_ENABLED" = true ]; then scripts/install-upx.sh; release_tool_paths; fi

run_linux_cli() {
  if [[ "$TARGET_TRIPLE" == aarch64-* ]]; then
    qemu-aarch64 "$1" --version
    qemu-aarch64 "$1" --help >/dev/null
  elif [[ "$TARGET_TRIPLE" == riscv64gc-* ]]; then
    qemu-riscv64 "$1" --version
    qemu-riscv64 "$1" --help >/dev/null
  else
    "$1" --version
    "$1" --help >/dev/null
  fi
}

case "$mode" in
  cli | headless)
    feature_args=()
    if [ "$mode" = headless ]; then
      export ARTIFACT_NAME="$(jq -r --arg target "$TARGET_TRIPLE" '.include[] | select(.target==$target) | .headless_artifact' scripts/release-targets.json)"
      export PACKAGE_INSTALLERS=false
      feature_args=(--no-default-features --features channels,memory,fs,bundled-vocabulary)
    fi
    if [ "$TARGET_OS" = macos ] || [[ "$TARGET_TRIPLE" == *-musl ]]; then
      if [[ "$TARGET_TRIPLE" == *-musl ]]; then export RUSTFLAGS='-C target-feature=+crt-static'; fi
      python3 scripts/reproducible-run.py cargo zigbuild --locked --release -p gproxy --bin gproxy --target "$TARGET_TRIPLE" "${feature_args[@]}"
      export BUILDER=cargo-zigbuild
    else
      python3 scripts/reproducible-run.py cargo build --locked --release -p gproxy --bin gproxy --target "$TARGET_TRIPLE" "${feature_args[@]}"
      export BUILDER=cargo-cross-gcc
    fi
    binary="target/$TARGET_TRIPLE/release/gproxy"
    if [ "$TARGET_OS" = linux ]; then
      mkdir -p "dist/container/$TARGET_TRIPLE"
      cp "$binary" "dist/container/$TARGET_TRIPLE/gproxy"
      run_linux_cli "$binary"
      upx_args=(--best --lzma)
      if [[ "$TARGET_TRIPLE" == riscv64gc-* ]]; then upx_args+=(--no-filter); fi
      upx "${upx_args[@]}" "$binary"
      upx --test "$binary"
      run_linux_cli "$binary"
    else
      rcodesign sign "$binary"
      python3 scripts/cnb/verify-macho.py "$binary"
    fi
    scripts/package-native-release.sh
    ;;
  application)
    export ARTIFACT_NAME="$APPLICATION_ARTIFACT" BUILDER=tauri-cross
    pnpm --dir crates/gproxy-host-tauri install --frozen-lockfile
    if [ "$TARGET_OS" = linux ]; then
      scripts/package-tauri-release.sh
    else
      config="$(jq -cn --arg version "$GPROXY_BUILD_VERSION" '{version:$version,bundle:{active:true}}')"
      (cd crates/gproxy-host-tauri && bash "$CI_PROJECT_DIR/scripts/with-tauri-desktop-lib.sh" pnpm exec tauri build --ci --runner cargo-zigbuild --target "$TARGET_TRIPLE" --no-bundle --config "$config" -- --locked)
      mkdir -p "dist/macos/$TARGET_TRIPLE"
      cp "target/$TARGET_TRIPLE/release/gproxy-desktop" "dist/macos/$TARGET_TRIPLE/"
    fi
    ;;
  *) exit 2 ;;
esac
scripts/build-provenance.sh
