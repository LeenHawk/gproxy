#!/usr/bin/env bash
# Builds gproxy's UPX fork from source, for builds that must not use a
# prebuilt packer: F-Droid, and the Android and OHOS toolchain images.
# scripts/install-upx.sh downloads the same revision's release binaries.
#
# LeenHawk/upx `gproxy` is upstream devel plus two fixes:
# - x86_64 Android shared libraries no longer trap when Android denies
#   /proc/self/auxv (upx/upx#18936; upstream fixed the entry stub only).
# - AArch64 Linux executables no longer die with SIGTRAP when the escape hatch
#   does not fit on the last page of .text: upx_mmap_and_fd had no body.
set -euo pipefail
revision=64b6aae95b9aa6d44992f8a6be422c2d77fc3728  # tag gproxy-5.3.0-1
source_dir="${1:-${RUNNER_TEMP:?}/gproxy-android-upx}"
git init "$source_dir"
if ! git -C "$source_dir" remote get-url origin >/dev/null 2>&1; then
  git -C "$source_dir" remote add origin https://github.com/LeenHawk/upx.git
fi
git -C "$source_dir" fetch --depth 1 origin "$revision"
git -C "$source_dir" checkout --force --detach FETCH_HEAD
git -C "$source_dir" submodule update --init --recursive --depth 1
cmake -S "$source_dir" -B "$source_dir/build" \
  -DCMAKE_BUILD_TYPE=Release -DUPX_CONFIG_DISABLE_WERROR=ON
cmake --build "$source_dir/build" --parallel 4
mkdir -p "$source_dir/bin"
install -m 755 "$source_dir/build/upx" "$source_dir/bin/upx"
if [ -n "${GITHUB_PATH:-}" ]; then
  printf '%s\n' "$source_dir/bin" >> "$GITHUB_PATH"
fi
"$source_dir/bin/upx" --version
