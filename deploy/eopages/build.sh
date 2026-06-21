#!/usr/bin/env bash
# Regenerate the wasm-bindgen `--target deno` glue for the EdgeOne Pages edge
# function entry, then patch the glue's loader tail from the
# streaming-fetch-from-URL form to a lazy base64-INLINE loader so the deployed
# bundle is fully self-contained (no sibling `.wasm` file to fetch at runtime).
#
# EdgeOne Pages Edge Functions are a V8 / Web-Service-Worker isolate that — as
# this spike empirically confirmed — exposes the `WebAssembly` global AND allows
# `WebAssembly.instantiate(bytes, imports)` (runtime byte compilation from a
# buffer). That is the SAME capability tier as Netlify / Supabase / Deno, so we
# reuse the deno-target glue + the base64-inline trick rather than the
# static-`?module`-import model that Cloudflare requires.
#
# Run from the crate root (/home/linhuan/gproxy/v2):
#   cargo build --lib --target wasm32-unknown-unknown --release --no-default-features --features edge
#   bash deploy/eopages/build.sh
set -euo pipefail

CRATE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WASM="$CRATE_ROOT/target/wasm32-unknown-unknown/release/gproxy.wasm"
OUT="$CRATE_ROOT/deploy/eopages/gproxy/edge-functions/_lib"

[ -f "$WASM" ] || { echo "missing $WASM — run cargo build first" >&2; exit 1; }

rm -rf "$OUT"
mkdir -p "$OUT"
OPT_WASM="$OUT/gproxy.optimized.wasm"
if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Oz --strip-debug --strip-producers "$WASM" -o "$OPT_WASM"
  BINDGEN_WASM="$OPT_WASM"
else
  echo "wasm-opt not found; using Cargo release wasm without post-link optimization" >&2
  BINDGEN_WASM="$WASM"
fi

wasm-bindgen --target deno --out-dir "$OUT" "$BINDGEN_WASM"
rm -f "$OPT_WASM"

# Emit the base64-inlined wasm module beside the glue.
INLINE="$OUT/gproxy_wasm_inline.ts"
{
  echo "// AUTO-GENERATED — do not edit. Inlined gproxy_bg.wasm (base64) so the"
  echo "// EdgeOne Pages edge-function bundle is self-contained (no static .wasm"
  echo "// file to fetch at runtime). Regenerate with deploy/eopages/build.sh."
  printf 'export const wasmBase64 = "'
  base64 -w0 "$OUT/gproxy_bg.wasm"
  printf '";\n'
} > "$INLINE"

# Rewrite the loader tail of the generated gproxy.js: replace the top-level
# streaming-fetch-from-URL instantiation with an explicit lazy loader. EdgeOne
# Pages appears to evaluate/bundle function modules together; top-level wasm
# instantiation can make the platform fall back to static assets before any
# handler runs.
perl -0pi -e '
  s~const wasmUrl = new URL\(.gproxy_bg\.wasm., import\.meta\.url\);\n.*?const wasm = wasmInstance\.exports;\n(?:wasm\.__wbindgen_start\(\);\n)?~
// EdgeOne Pages bundles this module; instantiate lazily from the inlined bytes\n// instead of fetching a sibling URL or doing top-level wasm work.\nimport { wasmBase64 } from "./gproxy_wasm_inline.ts";\nlet wasm;\nexport async function __gproxy_load() {\n    if (wasm) return;\n    const wasmBytes = Uint8Array.from(atob(wasmBase64), (c) => c.charCodeAt(0));\n    const wasmInstantiated = await WebAssembly.instantiate(wasmBytes, __wbg_get_imports());\n    const wasmInstance = wasmInstantiated.instance;\n    wasm = wasmInstance.exports;\n    if (wasm.__wbindgen_start) wasm.__wbindgen_start();\n}\n~s
' "$OUT/gproxy.js"

grep -q "__gproxy_load" "$OUT/gproxy.js" \
  && echo "patched $OUT/gproxy.js (lazy inline base64 loader)" \
  || { echo "PATCH FAILED — gproxy.js loader tail changed" >&2; exit 1; }

# Drop the now-unused sibling .wasm so it is not uploaded as a static asset.
rm -f "$OUT/gproxy_bg.wasm" "$OUT/gproxy_bg.wasm.d.ts"
echo "done: $OUT"
