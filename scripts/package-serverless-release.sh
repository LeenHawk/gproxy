#!/usr/bin/env bash
set -euo pipefail

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
rm -f "$output/$archive"
# Keep the executable's mode. No UPX: function sandboxes need an ordinary ELF.
zip -9 -q -j "$output/$archive" "$binary" LICENSE
(cd "$output" && sha256sum "$archive" > "$archive.sha256")
