#!/usr/bin/env bash
# The gproxy-lite bundle: the WebAssembly module and its JavaScript shell as
# the `lite/` directory every template in deploy/lite expects. The templates'
# prepare-lite.mjs downloads it, so a deploy button needs no Rust toolchain.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/reproducible-env.sh"

output_dir="${OUTPUT_DIR:-$PWD/dist/release}"
command -v wasm-opt >/dev/null || {
  echo "wasm-opt (Binaryen) is required for the release bundle" >&2
  exit 1
}

rm -rf deploy/lite/shell/pkg deploy/lite/cloudflare/lite
sh deploy/lite/build.sh

mkdir -p "$output_dir"
output_dir="$(cd "$output_dir" && pwd)"
archive="$output_dir/gproxy-lite.zip"
rm -f "$archive" "$archive.sha256"
# The templates share one shell and module; the Cloudflare copy is as good as any.
python3 scripts/reproducible-archive.py --root deploy/lite/cloudflare --output "$archive" lite
(cd "$output_dir" && sha256sum gproxy-lite.zip > gproxy-lite.zip.sha256)
