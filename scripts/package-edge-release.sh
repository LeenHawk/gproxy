#!/usr/bin/env bash
set -euo pipefail

console_dist="${CONSOLE_DIST:-$PWD/console/dist}"
output_dir="${OUTPUT_DIR:-$PWD/dist/release}"
raw_wasm="target/wasm32-unknown-unknown/release/gproxy_host_edge.wasm"

if [ ! -f "$console_dist/index.html" ]; then
  echo "missing prebuilt console: $console_dist" >&2
  exit 1
fi
command -v wasm-bindgen >/dev/null || {
  echo "wasm-bindgen CLI is required" >&2
  exit 1
}

cargo build --locked --release --package gproxy-host-edge \
  --target wasm32-unknown-unknown
if [ ! -f "$raw_wasm" ]; then
  echo "missing edge wasm build: $raw_wasm" >&2
  exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
web_pkg="$work/web-pkg"
rm -rf deploy/cloudflare/pkg
wasm-bindgen --target bundler --out-dir deploy/cloudflare/pkg \
  --out-name gproxy_host_edge "$raw_wasm"
wasm-bindgen --target web --out-dir "$web_pkg" \
  --out-name gproxy_host_edge "$raw_wasm"

for platform in cloudflare deno netlify; do
  rm -rf "deploy/$platform/public"
  mkdir -p "deploy/$platform/public"
  cp -R "$console_dist/." "deploy/$platform/public/"
done
for platform in deno netlify; do
  rm -rf "deploy/$platform/pkg"
  mkdir -p "deploy/$platform/pkg"
  cp -R "$web_pkg/." "deploy/$platform/pkg/"
done

mkdir -p "$output_dir"
output_dir="$(cd "$output_dir" && pwd)"
cp "$raw_wasm" "$output_dir/gproxy-edge.wasm"
for platform in cloudflare deno netlify; do
  archive="$output_dir/gproxy-edge-$platform.zip"
  rm -f "$archive" "$archive.sha256"
  (cd deploy && zip -9 -q -r "$archive" "$platform" \
    -x "$platform/node_modules/*" "$platform/.wrangler/*" "$platform/.netlify/*")
done
(cd "$output_dir" && for file in gproxy-edge.wasm gproxy-edge-*.zip; do
  sha256sum "$file" > "$file.sha256"
done)
