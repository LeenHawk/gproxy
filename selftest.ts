// Deno harness for the gproxy v2 wasm self-test.
//
// Loads the wasm-bindgen `deno`-target glue from ./pkg, reads the four test
// credentials from the environment (sourced from ./.env — never hard-coded
// here), then:
//   1. invokes the Rust `storage_selftest` export against the live Turso +
//      Upstash endpoints and prints the per-step summary it returns, and
//   2. exercises the INBOUND path — calls `init(...)` to build the real
//      AppState, then drives `fetch(new Request(...))` through the SAME
//      by-path dispatch the edge entry uses, asserting /healthz and /version.
//      Both endpoints are admin-gated (fail closed): without credentials the
//      expected answer is 401; set GPROXY_TEST_ADMIN_KEY to an enabled admin
//      user's API key to assert the authenticated 200 JSON bodies instead.
//      GPROXY_TEST_MASTER_KEY (optional) unseals encrypted stored secrets.
//
// Full reproduction (pkg/ is gitignored, regenerate it first):
//   cargo build --lib --target wasm32-unknown-unknown --release
//   wasm-bindgen --target deno --out-dir pkg \
//     target/wasm32-unknown-unknown/release/gproxy.wasm
//   # The crate also exports a fn named `fetch` (the WinterCG entry point),
//   # which shadows the global `fetch` that wasm-bindgen's deno loader uses to
//   # read the .wasm file, crashing at import ("Cannot access 'wasm' before
//   # initialization"). Force the loader to use the global explicitly:
//   perl -0pi -e \
//     's/instantiateStreaming\(fetch\(wasmUrl\)/instantiateStreaming(globalThis.fetch(wasmUrl)/' \
//     pkg/gproxy.js
//   set -a && source ./.env && set +a
//   deno run --allow-net --allow-env --allow-read selftest.ts

import { fetch as edgeFetch, init, storage_selftest } from "./pkg/gproxy.js";

function reqEnv(name: string): string {
  const v = Deno.env.get(name);
  if (!v) {
    console.error(`missing required env var: ${name}`);
    Deno.exit(1);
  }
  return v;
}

const tursoUrl = reqEnv("GPROXY_TEST_TURSO_URL");
const tursoToken = reqEnv("GPROXY_TEST_TURSO_TOKEN");
const upstashUrl = reqEnv("GPROXY_TEST_UPSTASH_URL");
const upstashToken = reqEnv("GPROXY_TEST_UPSTASH_TOKEN");
// Optional: master key (unseals encrypted secrets) and an admin user's API
// key (authenticates the admin-gated ops endpoints).
const masterKey = Deno.env.get("GPROXY_TEST_MASTER_KEY") || undefined;
const adminKey = Deno.env.get("GPROXY_TEST_ADMIN_KEY") || undefined;

// ── 1. storage backends ──────────────────────────────────────────────────────
const summary: string = await storage_selftest(
  tursoUrl,
  tursoToken,
  upstashUrl,
  upstashToken,
);

console.log("=== gproxy v2 edge storage self-test ===");
console.log(summary);

// ── 2. inbound path through the edge by-path dispatch ────────────────────────
console.log("\n=== gproxy v2 edge inbound self-test ===");

// Build the AppState once (persistence = libSQL/Turso, cache = Upstash).
await init(tursoUrl, tursoToken, upstashUrl, upstashToken, masterKey);

async function probe(path: string): Promise<string> {
  const headers = adminKey ? { "x-api-key": adminKey } : undefined;
  const resp = await edgeFetch(
    new Request(`https://gproxy.local${path}`, { headers }),
  );
  return `GET ${path} -> ${resp.status} ${await resp.text()}`;
}

const healthLine = await probe("/healthz");
const versionLine = await probe("/version");
console.log(healthLine);
console.log(versionLine);

// /healthz and /version are admin-gated and FAIL CLOSED: anonymous probes
// prove the gate (401); with GPROXY_TEST_ADMIN_KEY they prove the JSON bodies.
let ok = true;
if (adminKey) {
  if (!healthLine.endsWith('200 {"status":"ok"}')) {
    console.error(`ASSERT FAIL: /healthz expected 200 {"status":"ok"}`);
    ok = false;
  }
  if (!/-> 200 \{"version":".+"\}$/.test(versionLine)) {
    console.error(`ASSERT FAIL: /version expected 200 {"version":"..."}`);
    ok = false;
  }
} else {
  for (const line of [healthLine, versionLine]) {
    if (!/-> 401 /.test(line)) {
      console.error(
        `ASSERT FAIL: expected 401 without GPROXY_TEST_ADMIN_KEY, got: ${line}`,
      );
      ok = false;
    }
  }
}
console.log(ok ? "inbound: ALL OK" : "inbound: FAILED");
if (!ok) Deno.exit(1);
