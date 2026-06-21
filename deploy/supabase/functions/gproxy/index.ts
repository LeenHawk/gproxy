// GPROXY v2 — Supabase Edge Function entry.
//
// Loads the wasm-bindgen `deno`-target glue colocated in this function dir
// (gproxy.js + gproxy_bg.wasm — regenerated from the crate, see the build
// recipe below), wires the storage credentials from the function's secrets
// into the Rust `init(...)`, then serves every inbound request through the
// wasm `fetch` export (the SAME http::server::router native uses).
//
// Credentials are read from the function environment at module load — NEVER
// hard-coded here. Set them with `supabase secrets set ...`:
//   TURSO_URL, TURSO_TOKEN          (required — libSQL/Turso persistence)
//   UPSTASH_URL, UPSTASH_TOKEN      (optional — Upstash Redis cache; falls
//                                    back to the libSQL kv table when absent)
//   GPROXY_MASTER_KEY               (optional — unseals encrypted stored
//                                    secrets; absent → plaintext mode)
//
// Build recipe (run from the crate root; pkg/ is gitignored, so are the copies
// of the glue + .wasm placed alongside this file):
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
//   cp pkg/gproxy.js pkg/gproxy_bg.wasm pkg/*.d.ts deploy/supabase/functions/gproxy/
//
// Deploy from deploy/supabase/ (storage creds become function secrets; the
// access token is NOT):
//   supabase secrets set TURSO_URL=… TURSO_TOKEN=… UPSTASH_URL=… UPSTASH_TOKEN=… \
//     GPROXY_MASTER_KEY=… \
//     --project-ref "$SUPABASE_PROJECT_REF"
//   supabase functions deploy gproxy --project-ref "$SUPABASE_PROJECT_REF" \
//     --use-api --no-verify-jwt
//
// `wasmFetch` is aliased from the wasm `fetch` export so it does not shadow
// Deno's global `fetch`, which the glue's loader still needs at import time.

import { fetch as wasmFetch, init } from "./gproxy.js";

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

// Supabase routes invocations to `/<function-name>/<rest>` (it strips the
// `/functions/v1` mount but keeps the function name as a leading path segment).
// The wasm router matches bare paths (`/healthz`, `/version`), so peel the
// leading `/gproxy` segment off before handing the request to the router.
const FUNCTION_PREFIX = "/gproxy";

function stripFunctionPrefix(req: Request): Request {
  const url = new URL(req.url);
  if (url.pathname === FUNCTION_PREFIX) {
    url.pathname = "/";
  } else if (url.pathname.startsWith(FUNCTION_PREFIX + "/")) {
    url.pathname = url.pathname.slice(FUNCTION_PREFIX.length);
  }
  return new Request(url, req);
}

Deno.serve((req: Request) => wasmFetch(stripFunctionPrefix(req)));
