// GPROXY on Appwrite Functions — **deno-2.0 runtime** (pre-built wasm, no cargo).
//
// This wraps the SAME wasm-bindgen `--target deno` edge build that runs on
// Netlify / Supabase / Deno Deploy. Appwrite's open-runtimes deno runtime calls
// this default export per request; we bridge its `context.req` to the wasm
// `fetch` export (the same http::server::router native uses) and write the
// result to `context.res`.
//
// Storage credentials come from the function's environment variables (set them
// in the Appwrite Console → Function → Settings, or via the CLI):
//   TURSO_URL, TURSO_TOKEN          (required — libSQL/Turso control plane)
//   UPSTASH_URL, UPSTASH_TOKEN      (optional — Upstash cache; falls back to libSQL)
//   GPROXY_MASTER_KEY               (optional — unseals stored secrets)

import { fetch as wasmFetch, init } from "./gproxy.js";

function reqEnv(name: string): string {
  const v = Deno.env.get(name);
  if (!v) throw new Error(`GPROXY: missing required env ${name}`);
  return v;
}
function optEnv(name: string): string | undefined {
  const v = Deno.env.get(name);
  return v && v.length > 0 ? v : undefined;
}

// Build the control plane once per instance (cold start). Top-level await runs
// at module import, before the first request.
await init(
  reqEnv("TURSO_URL"),
  reqEnv("TURSO_TOKEN"),
  optEnv("UPSTASH_URL"),
  optEnv("UPSTASH_TOKEN"),
  optEnv("GPROXY_MASTER_KEY"),
);

// deno-lint-ignore no-explicit-any
export default async (context: any) => {
  const r = context.req;
  const method = (r.method || "GET").toUpperCase();
  const url = `https://gproxy.appwrite${r.path}${r.queryString ? "?" + r.queryString : ""}`;
  const hasBody = method !== "GET" && method !== "HEAD";
  const request = new Request(url, {
    method,
    headers: r.headers,
    body: hasBody ? r.bodyBinary : undefined,
  });

  const resp = await wasmFetch(request);

  const headers: Record<string, string> = {};
  resp.headers.forEach((v: string, k: string) => {
    headers[k] = v;
  });
  const body = new Uint8Array(await resp.arrayBuffer());
  return context.res.binary(body, resp.status, headers);
};
