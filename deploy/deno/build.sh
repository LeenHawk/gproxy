#!/usr/bin/env bash
# Build and deploy the Deno Deploy package.
#
# Deno Deploy's new platform stores the app build entrypoint. The verified app
# shape for gproxy-deno is a compact upload root with:
#   main.ts
#   pkg/gproxy.js
#   pkg/gproxy_bg.wasm
#   pkg/snippets/**  (wasm-bindgen inline_js modules)
# This script recreates that shape from the repo and deploys it.
set -euo pipefail

CRATE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
UPLOAD_ROOT="${TMPDIR:-/tmp}/gproxy-deno-upload"

: "${DENO_DEPLOY_TOKEN:?missing DENO_DEPLOY_TOKEN}"
: "${DENO_DEPLOY_PROJECT:=gproxy-deno}"
: "${DENO_DEPLOY_ORG:=leenhawk20}"

cd "$CRATE_ROOT"

cargo rustc --lib --crate-type cdylib --target wasm32-unknown-unknown --release --no-default-features --features edge
bash deploy/stage-edge-runtime.sh deno

rm -rf pkg
wasm-bindgen --target deno --out-dir pkg \
  target/wasm32-unknown-unknown/release/gproxy.wasm

grep -q "export function gproxyFetch" pkg/gproxy.js \
  || { echo "missing gproxyFetch export in generated glue" >&2; exit 1; }

rm -rf "$UPLOAD_ROOT"
mkdir -p "$UPLOAD_ROOT/pkg" "$UPLOAD_ROOT/console"
cp -R pkg/. "$UPLOAD_ROOT/pkg/"
cp deploy/deno/_shared.js "$UPLOAD_ROOT/_shared.js"
sed 's#../../pkg/gproxy.js#./pkg/gproxy.js#' deploy/deno/main.ts \
  > "$UPLOAD_ROOT/main.ts"
if [ -f assets/console/index.html ]; then
  cp -R assets/console/. "$UPLOAD_ROOT/console/"
else
  echo "warning: assets/console/index.html not found; Deno console assets not bundled" >&2
fi
cat > "$UPLOAD_ROOT/deno.json" <<JSON
{
  "deploy": {
    "org": "$DENO_DEPLOY_ORG",
    "app": "$DENO_DEPLOY_PROJECT",
    "include": ["deno.json", "main.ts", "_shared.js", "pkg/**", "console/**"]
  }
}
JSON

"${DENO_BIN:-$HOME/.deno/bin/deno}" run -A \
  https://jsr.io/@deno/deploy/0.0.99/main.ts \
  --token "$DENO_DEPLOY_TOKEN" \
  --prod \
  "$UPLOAD_ROOT"
