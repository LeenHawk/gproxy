#!/usr/bin/env bash
# Builds UPX from source, for builds that must not use a prebuilt packer:
# F-Droid, and the Android and OHOS toolchain images.
# scripts/install-upx.sh downloads release binaries of the same source.
#
# Upstream 5.3.0 is not enough: devel carries upx/upx#18968 (64-bit Linux
# executables no longer die at startup when the escape hatch does not fit on
# the last page of .text) and upx/upx#18970 (the RISC-V AUIPC filter matches
# the stub's unfilter and stays inside its buffer). Move to a release that
# contains both once upstream publishes one.
set -euo pipefail
revision=62c132272b994b69665a7f6b37871b3968c37f22  # upx/upx devel, 5.3.1
source_dir="${1:-${RUNNER_TEMP:?}/gproxy-android-upx}"
git init "$source_dir"
if ! git -C "$source_dir" remote get-url origin >/dev/null 2>&1; then
  git -C "$source_dir" remote add origin https://github.com/upx/upx.git
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
