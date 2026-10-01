#!/usr/bin/env bash
# The Cloudflare Workers bundle: `worker-build`'s Worker and the console as
# Workers Assets, in `deploy/cloudflare`, zipped so `wrangler deploy` works
# from the unpacked archive without a Rust toolchain.
set -euo pipefail

console_dist="${CONSOLE_DIST:-$PWD/console/dist}"
output_dir="${OUTPUT_DIR:-$PWD/dist/release}"
deploy="$PWD/deploy/cloudflare"

if [ ! -f "$console_dist/index.html" ]; then
  echo "missing prebuilt console: $console_dist" >&2
  exit 1
fi
command -v worker-build >/dev/null || {
  echo "worker-build is required (cargo install worker-build)" >&2
  exit 1
}

# Default features: every channel and D1. A deployment that wants a smaller
# Worker builds from source with the channels it forwards to.
#
# The workspace release profile strips symbols, and stripping also drops the
# `target_features` section wasm-bindgen reads to learn reference types are on:
# without it `worker-build`'s abort handler fails with "externref table
# required for catch wrappers". wasm-opt still shrinks the result.
(cd crates/gproxy-host-edge && CARGO_PROFILE_RELEASE_STRIP=none worker-build --release)
rm -rf "$deploy/build"
cp -R crates/gproxy-host-edge/build "$deploy/build"

rm -rf "$deploy/public"
mkdir -p "$deploy/public/console"
cp -R "$console_dist/." "$deploy/public/console/"
node console/scripts/prepare-edge-fonts.mjs "$deploy/public/console/index.html"
# The single-page fallback serves the root document.
cp "$deploy/public/console/index.html" "$deploy/public/index.html"
cp "$deploy/_headers" "$deploy/public/_headers"

mkdir -p "$output_dir"
output_dir="$(cd "$output_dir" && pwd)"
wasm="$(find "$deploy/build" -name '*.wasm' -print -quit)"
test -n "$wasm" || { echo "worker-build produced no wasm" >&2; exit 1; }
cp "$wasm" "$output_dir/gproxy-edge.wasm"
archive="$output_dir/gproxy-edge-cloudflare.zip"
rm -f "$archive" "$archive.sha256"
(cd deploy && zip -9 -q -r "$archive" cloudflare \
  -x "cloudflare/node_modules/*" "cloudflare/.wrangler/*")
(cd "$output_dir" && for file in gproxy-edge.wasm gproxy-edge-cloudflare.zip; do
  sha256sum "$file" > "$file.sha256"
done)
