#!/usr/bin/env bash
# Regenerate the wasm-bindgen `deno`-target glue for the Appwrite Functions
# (deno-2.0 runtime) entry, then inline the wasm as base64 so the function bundle
# is self-contained. Appwrite's deno runtime runs the SAME wasm as Netlify /
# Supabase / Deno Deploy — Appwrite never builds Rust, it just serves the
# pre-built wasm via main.ts.
#
# Build-only (no deploy/secrets). Run from the crate root (/home/linhuan/gproxy/v2):
#   cargo build --lib --target wasm32-unknown-unknown --release --no-default-features --features edge
#   bash deploy/appwrite-deno/build.sh
set -euo pipefail

CRATE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WASM="$CRATE_ROOT/target/wasm32-unknown-unknown/release/gproxy.wasm"
OUT="$CRATE_ROOT/deploy/appwrite-deno"

[ -f "$WASM" ] || { echo "missing $WASM — run cargo build first" >&2; exit 1; }

rm -f "$OUT/gproxy.js" "$OUT/gproxy.d.ts" "$OUT/gproxy_bg.wasm" "$OUT/gproxy_bg.wasm.d.ts" "$OUT/gproxy_wasm_inline.ts"
wasm-bindgen --target deno --out-dir "$OUT" "$WASM"

# Shrink (canonical order: wasm-opt AFTER wasm-bindgen), then base64-inline.
if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Oz -all --strip-debug --strip-producers "$OUT/gproxy_bg.wasm" -o "$OUT/gproxy_bg.opt.wasm"
  mv "$OUT/gproxy_bg.opt.wasm" "$OUT/gproxy_bg.wasm"
fi
{ printf 'export const wasmBase64 = "'; base64 -w0 "$OUT/gproxy_bg.wasm"; printf '";\n'; } > "$OUT/gproxy_wasm_inline.ts"

perl -0pi -e 's~const wasmUrl = new URL\(.gproxy_bg\.wasm., import\.meta\.url\);\nconst wasmInstantiated = await WebAssembly\.instantiateStreaming\(fetch\(wasmUrl\), (__wbg_get_imports\(\))\);~import { wasmBase64 } from "./gproxy_wasm_inline.ts";\nconst wasmBytes = Uint8Array.from(atob(wasmBase64), (c) => c.charCodeAt(0));\nconst wasmInstantiated = await WebAssembly.instantiate(wasmBytes, $1);~s' "$OUT/gproxy.js"

grep -q wasmBase64 "$OUT/gproxy.js" \
  && echo "patched $OUT/gproxy.js (inline base64 loader)" \
  || { echo "PATCH FAILED — gproxy.js loader tail changed" >&2; exit 1; }
rm -f "$OUT/gproxy_bg.wasm" "$OUT/gproxy_bg.wasm.d.ts"
echo "done: $OUT"
