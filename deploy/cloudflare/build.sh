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
#   cargo rustc --lib --crate-type cdylib --target wasm32-unknown-unknown --release --no-default-features --features edge
#   bash deploy/cloudflare/build.sh
set -euo pipefail

CRATE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WASM="$CRATE_ROOT/target/wasm32-unknown-unknown/release/gproxy.wasm"
OUT="$CRATE_ROOT/deploy/cloudflare/src/_lib"
CONSOLE_ASSETS="$CRATE_ROOT/assets/console"
PUBLIC="$CRATE_ROOT/deploy/cloudflare/public"
PUBLIC_CONSOLE="$PUBLIC/console"

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

mkdir -p "$PUBLIC_CONSOLE"
find "$PUBLIC" -mindepth 1 -maxdepth 1 ! -name ".gitkeep" ! -name "console" -exec rm -rf {} +
find "$PUBLIC_CONSOLE" -mindepth 1 ! -name ".gitkeep" -exec rm -rf {} +

if [ -f "$CONSOLE_ASSETS/index.html" ]; then
  cp -R "$CONSOLE_ASSETS"/. "$PUBLIC_CONSOLE"/
  cp "$CONSOLE_ASSETS/index.html" "$PUBLIC/__gproxy_console"
  for f in favicon.ico favicon-96x96.png apple-touch-icon.png; do
    if [ -f "$CONSOLE_ASSETS/$f" ]; then
      cp "$CONSOLE_ASSETS/$f" "$PUBLIC/$f"
    fi
  done
  echo "synced $CONSOLE_ASSETS -> $PUBLIC_CONSOLE"
else
  echo "warning: $CONSOLE_ASSETS/index.html not found; Cloudflare console assets not bundled" >&2
fi
