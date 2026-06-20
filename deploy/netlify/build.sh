#!/usr/bin/env bash
# Regenerate the wasm-bindgen `deno`-target glue for the Netlify Edge Function
# entry, then inline the wasm as base64 so the bundle is self-contained (no
# sibling .wasm to fetch at runtime). Netlify Edge Functions run on Deno Deploy
# infra, so this mirrors the eopages inline approach.
#
# Build-only (no deploy/secrets). Run from the crate root (/home/linhuan/gproxy/v2):
#   cargo build --lib --target wasm32-unknown-unknown --release --no-default-features --features edge
#   bash deploy/netlify/build.sh
set -euo pipefail

CRATE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WASM="$CRATE_ROOT/target/wasm32-unknown-unknown/release/gproxy.wasm"
OUT="$CRATE_ROOT/deploy/netlify/edge-functions/_lib"

[ -f "$WASM" ] || { echo "missing $WASM — run cargo build first" >&2; exit 1; }

rm -rf "$OUT"
mkdir -p "$OUT"
wasm-bindgen --target deno --out-dir "$OUT" "$WASM"

# Emit the base64-inlined wasm beside the glue.
INLINE="$OUT/gproxy_wasm_inline.ts"
{
  echo "// AUTO-GENERATED — do not edit. Inlined gproxy_bg.wasm (base64) so the"
  echo "// Netlify edge-function bundle is self-contained. Regenerate with"
  echo "// deploy/netlify/build.sh."
  printf 'export const wasmBase64 = "'
  base64 -w0 "$OUT/gproxy_bg.wasm"
  printf '";\n'
} > "$INLINE"

# Rewrite the loader: streaming-fetch-from-URL -> inline base64 instantiate.
perl -0pi -e '
  s~const wasmUrl = new URL\(.gproxy_bg\.wasm., import\.meta\.url\);\nconst wasmInstantiated = await WebAssembly\.instantiateStreaming\(fetch\(wasmUrl\), (__wbg_get_imports\(\))\);~import { wasmBase64 } from "./gproxy_wasm_inline.ts";\nconst wasmBytes = Uint8Array.from(atob(wasmBase64), (c) => c.charCodeAt(0));\nconst wasmInstantiated = await WebAssembly.instantiate(wasmBytes, $1);~s
' "$OUT/gproxy.js"

grep -q "wasmBase64" "$OUT/gproxy.js" \
  && echo "patched $OUT/gproxy.js (inline base64 loader)" \
  || { echo "PATCH FAILED — gproxy.js loader tail changed" >&2; exit 1; }

# Drop the now-unused sibling .wasm so it is not uploaded as a static asset.
rm -f "$OUT/gproxy_bg.wasm" "$OUT/gproxy_bg.wasm.d.ts"
echo "done: $OUT"
