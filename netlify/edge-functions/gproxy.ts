// GPROXY v2 — Netlify Edge Function entry.
//
// Netlify Edge Functions run on Deno Deploy infrastructure, so the same
// wasm-bindgen `--target deno` glue used for the Deno Deploy entries
// applies here. The wasm is INLINED as base64 (gproxy_wasm_inline.ts) and
// instantiated via `WebAssembly.instantiate(bytes, …)` so the function bundle
// is fully self-contained — no sibling `.wasm` file to fetch at runtime (this
// environment has no Docker and sibling-`.wasm` bundling is platform-dependent).
//
// The generated glue (gproxy.js + gproxy.d.ts + gproxy_wasm_inline.ts) is
// gitignored — only this file + netlify.toml are hand-written source.
//
// Required and optional variables are defined once in deploy/edge-runtime.js,
// copied into this entry's _lib/ by build.sh, and documented in deploy/README.md.
//
// Build recipe (run from the crate root; pkg/ and the generated glue copies in
// this dir are gitignored — regenerate after rebuilding the wasm):
//   cargo rustc --lib --crate-type cdylib --target wasm32-unknown-unknown --release --no-default-features --features edge
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
//   netlify env:set TURSO_URL …  (and TURSO_TOKEN / GPROXY_ADMIN_USER /
//                                 GPROXY_ADMIN_PASSWORD / UPSTASH_URL /
//                                 UPSTASH_TOKEN / GPROXY_MASTER_KEY)
//   netlify deploy --prod --dir public
//
// The Rust export deliberately uses a distinct JS name so the generated loader
// can keep using the runtime's global `fetch` during module initialisation.

// The generated glue lives in the `_lib/` subdirectory — Netlify only treats
// TOP-LEVEL files in the edge-functions dir as standalone functions, so nesting
// the glue keeps it as an imported module rather than a second "function".
import { gproxyFetch as wasmFetch, init } from "./_lib/gproxy.js";
import { createInitOnce, initGproxy } from "./_lib/_shared.js";

// Netlify exposes site env vars via the `Netlify.env` API on the edge runtime;
// fall back to `Deno.env` for parity with the Deno Deploy entries.
function getEnv(name: string): string | undefined {
  // deno-lint-ignore no-explicit-any
  const ne = (globalThis as any).Netlify?.env;
  const v = ne?.get?.(name) ?? Deno.env.get(name);
  return v && v.length > 0 ? v : undefined;
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
const ensureInit = createInitOnce(() => initGproxy(init, getEnv));

// The wasm router matches bare paths (`/healthz`, `/version`). Netlify Edge
// Functions invoke the handler with the ORIGINAL request URL (no synthetic
// function-name prefix, so the request path passes straight
// through to the router.
export default async (req: Request): Promise<Response> => {
  await ensureInit();
  return wasmFetch(req);
};
