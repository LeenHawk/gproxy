// GPROXY v2 — Netlify Edge Function entry.
//
// Netlify Edge Functions run on Deno Deploy infrastructure, so the same
// wasm-bindgen `--target deno` glue used for the Supabase / Deno Deploy entries
// applies here. The wasm is INLINED as base64 (gproxy_wasm_inline.ts) and
// instantiated via `WebAssembly.instantiate(bytes, …)` so the function bundle
// is fully self-contained — no sibling `.wasm` file to fetch at runtime (this
// environment has no Docker and sibling-`.wasm` bundling is platform-dependent).
//
// The generated glue (gproxy.js + gproxy.d.ts + gproxy_wasm_inline.ts) is
// gitignored — only this file + netlify.toml are hand-written source.
//
// Credentials are read from the site environment at module load — NEVER
// hard-coded here. Set them with `netlify env:set …`:
//   TURSO_URL, TURSO_TOKEN          (required — libSQL/Turso persistence)
//   UPSTASH_URL, UPSTASH_TOKEN      (optional — Upstash Redis cache; falls
//                                    back to the libSQL kv table when absent)
//   GPROXY_MASTER_KEY               (optional — unseals encrypted stored
//                                    secrets; absent → plaintext mode)
//
// Build recipe (run from the crate root; pkg/ and the generated glue copies in
// this dir are gitignored — regenerate after rebuilding the wasm):
//   cargo build --lib --target wasm32-unknown-unknown --release --no-default-features --features edge
//   wasm-bindgen --target deno --out-dir pkg \
//     target/wasm32-unknown-unknown/release/gproxy.wasm
//   cp pkg/gproxy.js pkg/gproxy.d.ts deploy/netlify/edge-functions/_lib/
//   # Generate the base64 inline module from pkg/gproxy_bg.wasm into
//   # deploy/netlify/edge-functions/_lib/gproxy_wasm_inline.ts and rewrite the loader
//   # tail of the copied gproxy.js from the streaming-fetch-from-URL form
//   #   const wasmUrl = new URL('gproxy_bg.wasm', import.meta.url);
//   #   …WebAssembly.instantiateStreaming(fetch(wasmUrl), __wbg_get_imports());
//   # to the inline form:
//   #   import { wasmBase64 } from "./gproxy_wasm_inline.ts";
//   #   const wasmBytes = Uint8Array.from(atob(wasmBase64), c => c.charCodeAt(0));
//   #   …WebAssembly.instantiate(wasmBytes, __wbg_get_imports());
//
// Deploy from deploy/netlify/ (storage creds become site env vars; the Netlify
// auth token is NOT):
//   netlify env:set TURSO_URL …  (and TURSO_TOKEN / UPSTASH_URL / UPSTASH_TOKEN
//                                 / GPROXY_MASTER_KEY)
//   netlify deploy --prod --dir public
//
// `wasmFetch` is aliased from the wasm `fetch` export so it does not shadow the
// runtime's global `fetch`.

// The generated glue lives in the `_lib/` subdirectory — Netlify only treats
// TOP-LEVEL files in the edge-functions dir as standalone functions, so nesting
// the glue keeps it as an imported module rather than a second "function".
import { fetch as wasmFetch, init } from "./_lib/gproxy.js";

// Netlify exposes site env vars via the `Netlify.env` API on the edge runtime;
// fall back to `Deno.env` for parity with the Supabase / Deno Deploy entries.
function getEnv(name: string): string | undefined {
  // deno-lint-ignore no-explicit-any
  const ne = (globalThis as any).Netlify?.env;
  const v = ne?.get?.(name) ?? Deno.env.get(name);
  return v && v.length > 0 ? v : undefined;
}

function reqEnv(name: string): string {
  const v = getEnv(name);
  if (!v) {
    throw new Error(`missing required env var: ${name}`);
  }
  return v;
}

// Build the shared AppState exactly once, LAZILY on the first request.
//
// Netlify's edge bundler IMPORTS this module at build time to validate the
// default export, executing any top-level code. The storage env vars are NOT
// injected during that bundling pass, so a top-level `await init(...)` would
// throw "missing required env var: TURSO_URL" and fail the build. Deferring
// init to the first invocation (where `Netlify.env` / `Deno.env` are populated)
// avoids that while still initialising only once (the promise is memoised; the
// Rust `init` is itself idempotent — the first AppState wins).
let initialised: Promise<void> | undefined;

function ensureInit(): Promise<void> {
  if (!initialised) {
    initialised = init(
      reqEnv("TURSO_URL"),
      reqEnv("TURSO_TOKEN"),
      getEnv("UPSTASH_URL"),
      getEnv("UPSTASH_TOKEN"),
      getEnv("GPROXY_MASTER_KEY"),
    );
  }
  return initialised;
}

// The wasm router matches bare paths (`/healthz`, `/version`). Netlify Edge
// Functions invoke the handler with the ORIGINAL request URL (no synthetic
// function-name prefix, unlike Supabase), so the request path passes straight
// through to the router.
export default async (req: Request): Promise<Response> => {
  await ensureInit();
  return wasmFetch(req);
};
