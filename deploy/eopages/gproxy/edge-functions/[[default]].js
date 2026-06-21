// GPROXY v2 — EdgeOne Pages edge-function entry: ROOT catch-all.
//
// Every path (aggregated /v1/*, scoped /{provider}/v1/*, and the admin-gated
// ops endpoints /healthz /version /metrics) routes to the wasm fetch dispatch
// here — the same shape as every other platform entry. `/` is the one
// exception: the static index.html exact-match outranks the catch-all.
//
// REQUIRES edgeone CLI >= 1.5.9: earlier versions had a routing bug where
// [[default]].js either never registered on direct uploads or swallowed all
// routes — probed and confirmed fixed on 1.6.1 (2026-06-12, see
// ../../NOTES.md "Routing shapes").
//
// Env vars come from `context.env` (set with `edgeone pages env set …`):
//   TURSO_URL, TURSO_TOKEN          (required — libSQL/Turso persistence)
//   UPSTASH_URL, UPSTASH_TOKEN      (optional — Upstash Redis cache)
//   GPROXY_MASTER_KEY               (optional — unseals encrypted stored
//                                    secrets; absent → plaintext mode)
import {
  __gproxy_load,
  fetch as wasmFetch,
  init,
} from "./_lib/gproxy.js";

let ready;

function reqEnv(env, name) {
  const v = env && env[name];
  if (!v) {
    throw new Error(`missing required env var: ${name}`);
  }
  return v;
}

function optEnv(env, name) {
  const v = env && env[name];
  return v && v.length > 0 ? v : undefined;
}

function ensureInit(env) {
  if (!ready) {
    ready = (async () => {
      await __gproxy_load();
      await init(
        reqEnv(env, "TURSO_URL"),
        reqEnv(env, "TURSO_TOKEN"),
        optEnv(env, "UPSTASH_URL"),
        optEnv(env, "UPSTASH_TOKEN"),
        optEnv(env, "GPROXY_MASTER_KEY"),
      );
    })();
  }
  return ready;
}

export async function onRequest(context) {
  try {
    await ensureInit(context.env);
    return wasmFetch(context.request);
  } catch (e) {
    return new Response("edge-init-error\n", {
      status: 500,
      headers: { "content-type": "text/plain" },
    });
  }
}
