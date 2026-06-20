#!/usr/bin/env bash
# Regenerate the wasm-bindgen `deno`-target glue for the Supabase Edge Function
# entry, colocated next to index.ts (gitignored build output). The crate exports
# a `fetch` fn (WinterCG entry) that shadows the global `fetch` the deno loader
# needs at import time; force the loader to use globalThis.fetch explicitly
# (same fix as deploy/deno/build.sh).
#
# Build-only (no deploy/secrets). Run from the crate root (/home/linhuan/gproxy/v2):
#   cargo build --lib --target wasm32-unknown-unknown --release --no-default-features --features edge
#   bash deploy/supabase/build.sh
set -euo pipefail

CRATE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WASM="$CRATE_ROOT/target/wasm32-unknown-unknown/release/gproxy.wasm"
OUT="$CRATE_ROOT/deploy/supabase/functions/gproxy"

[ -f "$WASM" ] || { echo "missing $WASM — run cargo build first" >&2; exit 1; }

rm -f "$OUT/gproxy.js" "$OUT/gproxy.d.ts" "$OUT/gproxy_bg.wasm" "$OUT/gproxy_bg.wasm.d.ts"
wasm-bindgen --target deno --out-dir "$OUT" "$WASM"

# The crate's exported `fetch` shadows the global the deno loader uses to read
# the sibling .wasm at import — force globalThis.fetch.
perl -0pi -e \
  's/instantiateStreaming\(fetch\(wasmUrl\)/instantiateStreaming(globalThis.fetch(wasmUrl)/' \
  "$OUT/gproxy.js"

grep -q "globalThis.fetch(wasmUrl)" "$OUT/gproxy.js" \
  && echo "patched $OUT/gproxy.js (globalThis.fetch)" \
  || { echo "PATCH FAILED — gproxy.js loader tail changed" >&2; exit 1; }
echo "done: $OUT"
