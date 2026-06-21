#!/usr/bin/env bash
# Build and deploy the Deno Deploy package.
#
# Deno Deploy's new platform stores the app build entrypoint. The verified app
# shape for gproxy-deno is a compact upload root with:
#   main.ts
#   pkg/gproxy.js
#   pkg/gproxy_bg.wasm
# This script recreates that shape from the repo and deploys it.
set -euo pipefail

CRATE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
UPLOAD_ROOT="${TMPDIR:-/tmp}/gproxy-deno-upload"

: "${DENO_DEPLOY_TOKEN:?missing DENO_DEPLOY_TOKEN}"
: "${DENO_DEPLOY_PROJECT:=gproxy-deno}"
: "${DENO_DEPLOY_ORG:=leenhawk20}"

cd "$CRATE_ROOT"

cargo build --lib --target wasm32-unknown-unknown --release --no-default-features --features edge

rm -rf pkg
wasm-bindgen --target deno --out-dir pkg \
  target/wasm32-unknown-unknown/release/gproxy.wasm

perl -0pi -e \
  's/instantiateStreaming\(fetch\(wasmUrl\)/instantiateStreaming(globalThis.fetch(wasmUrl)/' \
  pkg/gproxy.js

rm -rf "$UPLOAD_ROOT"
mkdir -p "$UPLOAD_ROOT/pkg"
cp pkg/gproxy.js pkg/gproxy.d.ts pkg/gproxy_bg.wasm pkg/gproxy_bg.wasm.d.ts \
  "$UPLOAD_ROOT/pkg/"
sed 's#../../pkg/gproxy.js#./pkg/gproxy.js#' deploy/deno/main.ts \
  > "$UPLOAD_ROOT/main.ts"
cat > "$UPLOAD_ROOT/deno.json" <<JSON
{
  "deploy": {
    "org": "$DENO_DEPLOY_ORG",
    "app": "$DENO_DEPLOY_PROJECT",
    "include": ["main.ts", "pkg/**"]
  }
}
JSON

"${DENO_BIN:-$HOME/.deno/bin/deno}" run -A \
  https://jsr.io/@deno/deploy/0.0.99/main.ts \
  --token "$DENO_DEPLOY_TOKEN" \
  --prod \
  "$UPLOAD_ROOT"
