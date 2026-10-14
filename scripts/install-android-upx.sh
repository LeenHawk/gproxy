#!/usr/bin/env bash
# UPX 5.2.1 cannot pack our ARM64 shared library (upx/upx#18916).
# Use the upstream Android shared-library stub fix until it is released.
# UPX also mishandles x86_64 --android-shlib output (upx/upx#18936). The
# adjacent patch keeps 16 KiB PT_LOAD alignment, forwards section headers as
# for ARM64, and lets the x86_64 stubs fall back to 4 KiB pages when Android
# denies /proc/self/auxv instead of trapping. Its stub headers were rebuilt
# with upx-stubtools 20221212; keep the patch beside this script.
set -euo pipefail
revision=d5faee0886bb35ea24d7ff07b5bab518b9153796
source_dir="${1:-${RUNNER_TEMP:?}/gproxy-android-upx}"
git init "$source_dir"
if ! git -C "$source_dir" remote get-url origin >/dev/null 2>&1; then
  git -C "$source_dir" remote add origin https://github.com/upx/upx.git
fi
git -C "$source_dir" fetch --depth 1 origin "$revision"
git -C "$source_dir" checkout --force --detach FETCH_HEAD
git -C "$source_dir" submodule update --init --recursive --depth 1
git -C "$source_dir" apply - < "${BASH_SOURCE[0]%.sh}.patch"
cmake -S "$source_dir" -B "$source_dir/build" \
  -DCMAKE_BUILD_TYPE=Release -DUPX_CONFIG_DISABLE_WERROR=ON
cmake --build "$source_dir/build" --parallel 4
mkdir -p "$source_dir/bin"
install -m 755 "$source_dir/build/upx" "$source_dir/bin/upx"
if [ -n "${GITHUB_PATH:-}" ]; then
  printf '%s\n' "$source_dir/bin" >> "$GITHUB_PATH"
fi
"$source_dir/bin/upx" --version
