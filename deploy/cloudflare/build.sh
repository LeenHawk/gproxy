#!/usr/bin/env bash
# Regenerate the wasm-bindgen `--target web` glue for the Cloudflare Workers
# entry, then patch the glue so the worker injects the bundler-provided
# `WebAssembly.Module` instead of fetching the .wasm by URL at runtime.
#
# Cloudflare Workers use a static-wasm-module model: you
# `import wasm from "./gproxy_bg.wasm"` and wrangler bundles it as a
# `WebAssembly.Module` (no `?module` suffix, no runtime byte compilation). The
# web-target default export (`__wbg_init`) routes a `WebAssembly.Module` straight
# to `WebAssembly.instantiate(module, imports)`, which is exactly what CF wants.
#
# The handler (src/worker.js) ALWAYS passes that statically-imported Module to
# the loader, so the loader's URL-fetch fallback
# (`new URL('gproxy_bg.wasm', import.meta.url)`) is dead code at runtime. We
# replace it with a throw so wrangler never tries to resolve the .wasm via a URL
# (which would fail in the Workers module sandbox).
#
# Run from the crate root (/home/linhuan/gproxy/v2):
#   cargo build --lib --target wasm32-unknown-unknown --release --no-default-features --features edge
#   bash deploy/cloudflare/build.sh
set -euo pipefail

CRATE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WASM="$CRATE_ROOT/target/wasm32-unknown-unknown/release/gproxy.wasm"
OUT="$CRATE_ROOT/deploy/cloudflare/src/_lib"

[ -f "$WASM" ] || { echo "missing $WASM — run cargo build first" >&2; exit 1; }

rm -rf "$OUT"
wasm-bindgen --target web --out-dir "$OUT" "$WASM"

# Drop the URL-fetch fallback so wrangler does not try to resolve the .wasm via
# `new URL(...)` (the worker always injects the bundled Module explicitly).
perl -0pi -e \
  "s/module_or_path = new URL\('gproxy_bg\.wasm', import\.meta\.url\);/throw new Error('pass the WebAssembly.Module explicitly (Cloudflare Workers: no URL fetch of the .wasm)');/" \
  "$OUT/gproxy.js"

grep -q "no URL fetch of the .wasm" "$OUT/gproxy.js" \
  && echo "patched $OUT/gproxy.js (removed gproxy_bg.wasm URL fallback)" \
  || { echo "PATCH FAILED — gproxy.js loader tail changed" >&2; exit 1; }
