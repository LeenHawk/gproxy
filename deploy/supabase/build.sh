#!/usr/bin/env bash
# Regenerate the wasm-bindgen `deno`-target glue for the Supabase Edge Function
# entry (colocated next to index.ts), then inline the wasm as base64 so the
# bundle is SELF-CONTAINED.
#
# Why inline (not a sibling .wasm): `supabase functions deploy --use-api` only
# uploads the .ts/.js sources, not a sibling gproxy_bg.wasm — so a fetch-the-
# sibling loader crashes at module load (WORKER_ERROR). Supabase Edge runs on
# Deno, same capability tier as netlify/eopages, so the base64-inline loader
# (runtime WebAssembly.instantiate of the decoded bytes) is the portable form.
#
# Build-only (no deploy/secrets). Run from the crate root (/home/linhuan/gproxy/v2):
#   cargo build --lib --target wasm32-unknown-unknown --release --no-default-features --features edge
#   bash deploy/supabase/build.sh
set -euo pipefail

CRATE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WASM="$CRATE_ROOT/target/wasm32-unknown-unknown/release/gproxy.wasm"
OUT="$CRATE_ROOT/deploy/supabase/functions/gproxy"

[ -f "$WASM" ] || { echo "missing $WASM — run cargo build first" >&2; exit 1; }

rm -f "$OUT/gproxy.js" "$OUT/gproxy.d.ts" "$OUT/gproxy_bg.wasm" "$OUT/gproxy_bg.wasm.d.ts" "$OUT/gproxy_wasm_inline.ts"
wasm-bindgen --target deno --out-dir "$OUT" "$WASM"

# Shrink the bindgen output to fit Supabase's deploy payload limit. Canonical
# order — wasm-opt runs AFTER wasm-bindgen (on the final module), so it never
# touches the bindgen descriptor functions. Optional: falls back to the
# un-optimized wasm if wasm-opt is absent.
if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Oz -all --strip-debug --strip-producers "$OUT/gproxy_bg.wasm" -o "$OUT/gproxy_bg.opt.wasm"
  mv "$OUT/gproxy_bg.opt.wasm" "$OUT/gproxy_bg.wasm"
else
  echo "wasm-opt not found; inlining un-optimized wasm (may exceed Supabase's deploy size limit)" >&2
fi

# Emit the base64-inlined wasm beside the glue.
INLINE="$OUT/gproxy_wasm_inline.ts"
{
  echo "// AUTO-GENERATED — do not edit. Inlined gproxy_bg.wasm (base64) so the"
  echo "// Supabase edge-function bundle is self-contained. Regenerate with"
  echo "// deploy/supabase/build.sh."
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

# Drop the now-unused sibling .wasm.
rm -f "$OUT/gproxy_bg.wasm" "$OUT/gproxy_bg.wasm.d.ts"
echo "done: $OUT"
