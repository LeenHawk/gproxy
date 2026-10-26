#!/usr/bin/env bash
# F-Droid prebuild: pinned FLOSS build tools, outside the scanned app checkout.
set -euo pipefail
# Match the upstream reproducibility build before installing Rust or Cargo tools.
build_root=/tmp/gproxy-reproduce
(umask 077; mkdir "$build_root")
tools_dir="$build_root/tools"
export RUSTUP_HOME="$tools_dir/rustup" CARGO_HOME="$build_root/cargo"
mkdir -p "$tools_dir" "$CARGO_HOME"
source scripts/mobile/android-toolchain.sh
install_android_toolchain "$tools_dir"
bash scripts/install-android-upx.sh "$tools_dir/gproxy-android-upx"
