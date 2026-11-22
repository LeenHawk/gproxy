#!/bin/sh
# Build the gproxy-lite WebAssembly module and place it, with the JavaScript
# shell, in each platform template. Every channel is built in.
#
# Needs the wasm32-unknown-unknown target and a wasm-bindgen CLI of the
# version in Cargo.lock. wasm-opt is used when it is on PATH.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
pkg="$here/shell/pkg"

cd "$root"
# `cdylib` only here: a native build of the workspace has no use for one.
cargo rustc --release --target wasm32-unknown-unknown -p gproxy-lite --features all-channels \
    --crate-type cdylib
wasm-bindgen --target web --no-typescript --out-dir "$pkg" \
    target/wasm32-unknown-unknown/release/gproxy_lite.wasm
if command -v wasm-opt >/dev/null; then
    # The features rustc's wasm32 baseline emits; wasm-opt assumes none.
    wasm-opt -Oz --enable-bulk-memory --enable-sign-ext --enable-mutable-globals \
        --enable-nontrapping-float-to-int --enable-reference-types --enable-multivalue \
        -o "$pkg/optimized.wasm" "$pkg/gproxy_lite_bg.wasm"
    mv "$pkg/optimized.wasm" "$pkg/gproxy_lite_bg.wasm"
fi

for platform in cloudflare netlify vercel; do
    mkdir -p "$here/$platform/lite/stores" "$here/$platform/lite/pkg"
    cp "$here"/shell/*.mjs "$here/$platform/lite/"
    cp "$here"/shell/stores/*.mjs "$here/$platform/lite/stores/"
    cp "$pkg"/gproxy_lite.js "$pkg"/gproxy_lite_bg.wasm "$here/$platform/lite/pkg/"
done
ls -l "$pkg/gproxy_lite_bg.wasm"
