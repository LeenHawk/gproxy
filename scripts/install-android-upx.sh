#!/usr/bin/env bash
# Builds gproxy's UPX fork from source, for builds that must not use a
# prebuilt packer: F-Droid, and the Android and OHOS toolchain images.
# scripts/install-upx.sh downloads the same revision's release binaries.
#
# LeenHawk/upx `gproxy` is upstream devel plus two pull requests:
# - upx/upx#18968: 64-bit Linux executables no longer die at startup when the
#   escape hatch does not fit on the last page of .text (x86_64, RISC-V,
#   PowerPC64; AArch64 is already fixed upstream).
# - upx/upx#18970: the RISC-V AUIPC filter matches the stub's unfilter, stays
#   inside its buffer and accepts gangs of any length.
set -euo pipefail
revision=da64eda7aee2d43f91fc0ae43e6404235579d0c7  # tag gproxy-5.3.0-2
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
