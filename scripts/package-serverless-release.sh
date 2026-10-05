#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/reproducible-env.sh"

target="${TARGET_TRIPLE:?missing TARGET_TRIPLE}"
case "$target" in
  x86_64-unknown-linux-musl) arch=x86_64 ;;
  aarch64-unknown-linux-musl) arch=aarch64 ;;
  *) echo "unsupported serverless target: $target" >&2; exit 1 ;;
esac
binary="$PWD/target/$target/release/gproxy-serverless"
output="${OUTPUT_DIR:-$PWD/dist/release}"
mkdir -p "$output"
output="$(cd "$output" && pwd)"
archive="gproxy-serverless-linux-$arch-musl.zip"
test -f "$binary"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
install -m755 "$binary" "$work/gproxy-serverless"
upx --best --lzma "$work/gproxy-serverless"
upx --test "$work/gproxy-serverless"
"$work/gproxy-serverless" --version
rm -f "$output/$archive"
# Compress the staged executable without changing Cargo's cached output.
install -m644 LICENSE "$work/LICENSE"
python3 scripts/reproducible-archive.py --root "$work" --output "$output/$archive"
(cd "$output" && sha256sum "$archive" > "$archive.sha256")
