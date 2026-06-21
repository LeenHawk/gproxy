// GPROXY v2 — Deno Deploy production entry.
//
// Loads the wasm-bindgen `deno`-target glue from ../../pkg (regenerated, never
// committed — see the build recipe below), wires the storage credentials from
// the Deno Deploy environment into the Rust `init(...)`, then serves every
// inbound request through the wasm `fetch` export (the SAME http::server::router
// native uses).
//
// Credentials are read from Deno Deploy env vars at module load — NEVER hard-
// coded here:
//   TURSO_URL, TURSO_TOKEN          (required — libSQL/Turso persistence)
//   UPSTASH_URL, UPSTASH_TOKEN      (optional — Upstash Redis cache; falls
//                                    back to the libSQL kv table when absent)
//   GPROXY_MASTER_KEY               (optional — unseals encrypted stored
//                                    secrets; absent → plaintext mode)
//
// Build recipe (run from the crate root before deploying; pkg/ is gitignored):
//   cargo build --lib --target wasm32-unknown-unknown --release --no-default-features --features edge
//   wasm-bindgen --target deno --out-dir pkg \
//     target/wasm32-unknown-unknown/release/gproxy.wasm
//   # The crate exports a fn named `fetch` (the WinterCG entry point), which
//   # shadows the global `fetch` that wasm-bindgen's deno loader uses to read
//   # the .wasm at import ("Cannot access 'wasm' before initialization"). Force
//   # the loader to use the global explicitly:
//   perl -0pi -e \
//     's/instantiateStreaming\(fetch\(wasmUrl\)/instantiateStreaming(globalThis.fetch(wasmUrl)/' \
//     pkg/gproxy.js
//
// Deploy with deploy/deno/build.sh. The script builds a temporary upload root
// whose main.ts imports ./pkg/gproxy.js, matching Deno Deploy's app build
// configuration.
//
// `wasmFetch` is aliased from the wasm `fetch` export so it does not shadow
// Deno's global `fetch`, which the glue's loader still needs at import time.

import { fetch as wasmFetch, init } from "../../pkg/gproxy.js";

function reqEnv(name: string): string {
  const v = Deno.env.get(name);
  if (!v) {
    throw new Error(`missing required env var: ${name}`);
  }
  return v;
}

function optEnv(name: string): string | undefined {
  const v = Deno.env.get(name);
  return v && v.length > 0 ? v : undefined;
}

// Build the shared AppState once, on module load.
await init(
  reqEnv("TURSO_URL"),
  reqEnv("TURSO_TOKEN"),
  optEnv("UPSTASH_URL"),
  optEnv("UPSTASH_TOKEN"),
  optEnv("GPROXY_MASTER_KEY"),
);

Deno.serve((req: Request) => wasmFetch(req));
