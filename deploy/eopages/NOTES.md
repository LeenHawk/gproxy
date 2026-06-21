# Tencent EdgeOne **Pages** — WASM Edge Function Feasibility Spike

**Branch:** `phase-1k-edgeone-pages`
**Date:** 2026-06-04
**Product:** EdgeOne **Pages** (the full-stack Pages product, with its OWN
`edgeone` npm CLI + Pages API token) — DISTINCT from the EdgeOne CDN `teo`
2022-09-01 API explored in the earlier `phase-1i-edgeone-spike`, which found
the account had no `teo` zone.
**Question:** Can GPROXY v2's WebAssembly edge build deploy + run on EdgeOne Pages?

## STATUS: `GPROXY_WASM_LIVE_WITH_CONSTRAINTS`

EdgeOne Pages edge functions **DO run WebAssembly**, and GPROXY v2's real wasm
edge build now deploys and serves live routes on EdgeOne Pages. The first
attempt with a ~612 KB `wasm-bindgen` module was killed during
`WebAssembly.instantiate()` at a fixed **~15.0 s**. Step 1 fixed it by shrinking
the release wasm and making the inline wasm loader explicit/lazy:

- Cargo release profile: `opt-level = "z"`, `lto = "fat"`, `codegen-units = 1`,
  `panic = "abort"`, `strip = true`.
- `deploy/eopages/build.sh` optionally runs `wasm-opt -Oz` when available, then
  base64-inlines the generated wasm.
- The generated glue exports `__gproxy_load()`, so EdgeOne does not instantiate
  wasm as top-level module work before a handler runs.
- GPROXY uses explicit route files (`healthz.js`, `version.js`) instead of the
  root `[[default]].js` catch-all, because that catch-all fell back to static
  assets in direct uploads.

Latest live proof on `gproxy-v2`: a temporary `/loadprobe` route returned
`load-ok ms=65`; after removing temporary probes and redeploying the cleaned
package, `/healthz` returned `200 {"status":"ok"}` in 0.795 s and `/version`
returned `200 {"version":"2.0.0"}` in 0.338 s.

## Routing shapes (2026-06-12): root catch-all WORKS on CLI ≥ 1.5.9 → FULL gateway

The Step-4 "root `[[default]].js` falls back to static assets" finding was a
**CLI/platform routing bug, fixed in 1.5.9** (confirmed by the EdgeOne team:
"[[\*]] 吃所有路由的问题,1.5.9 修复了"; `/` falling to `index.html` is correct —
exact static matches outrank the catch-all). Probed on `gproxy-spike`:

| shape | CLI 1.5.6 | CLI 1.6.1 |
|---|---|---|
| `healthz.js` (exact file) | ✅ | ✅ |
| `v1/[[default]].js` (static dir + catch-all) | ✅ | ✅ |
| `[seg]/echo.js` (dynamic dir + static file) | ✅ | ✅ |
| `[[default]].js` (ROOT catch-all) | ❌ static fallback | ✅ `/anything`, `/openai/v1/messages`, bare `/v1` all hit it |
| `[seg]/[[default]].js`, `[seg]/v1/[[default]].js` (dynamic dir + catch-all) | ❌ | ❌ (root catch-all covers them instead) |
| `/` | static `index.html` | static `index.html` (exact match wins — by design) |

Precedence observed: exact file > static-dir catch-all > dynamic-dir static
file > ROOT catch-all > static assets (only `/` exact-matches one).

So the GPROXY package is now a SINGLE root `edge-functions/[[default]].js`
routing **everything** — aggregated `/v1/*`, scoped `/{provider}/v1/*`, and the
admin-gated ops endpoints — into the wasm fetch dispatch, the same shape as
every other platform entry. The explicit healthz/version/metrics route files
and the interim `v1/[[default]].js` are gone. **Requires `edgeone` CLI ≥ 1.5.9
to deploy** (earlier versions mis-register catch-alls).

Remaining live verification (one deploy — `gproxy-v2` already carries the
TURSO/UPSTASH env vars from the Step-4 spike):

```
edgeone pages deploy deploy/eopages/gproxy --name gproxy-v2 -t "$EDGEONE_PAGES_API_TOKEN"
# then, with the eo_token/eo_time pair from the deploy output:
#   /v1/models        -> gproxy 401 JSON (pipeline reached, API key required)
#   /openai/v1/models -> gproxy 401 JSON (scoped path reached)
#   /healthz          -> 401 unauthorized (admin gate)
#   /                 -> static index.html
```

Also note (2026-06-12): the ops endpoints are now admin-gated FAIL CLOSED —
probes must send an admin user's API key (`x-api-key` / `Authorization:
Bearer`) or an admin session cookie; anonymous curl gets 401. The live-proof
transcripts in this file predate that gate (and the JSON bodies they show are
now served by the edge dispatch itself, matching native byte-for-byte).

---

## Tooling / CLI

- **CLI:** `edgeone` **v1.6.1** (`npm i -g edgeone`, node v22.20.0; **≥ 1.5.9
  required** — see "Routing shapes" above; 1.6.1 warns `"edgeone pages" is
  deprecated. Use "edgeone makers" instead`, but `pages` still works).
  Commands: `login | whoami | switch | logout | pages {init,dev,
  generate-routes,env,link,deploy}`.
- **Auth (non-interactive): the CLI reads `EDGEONE_PAGES_API_TOKEN` from the
  environment directly** — no `edgeone login` needed. Proven in a clean env
  (`env -i` carrying ONLY that var): `edgeone whoami` → authenticated as
  account `100016841661` / APPID `1304789703`. `edgeone login --token …` is NOT
  required; `pages deploy/link/env` also accept `-t/--token`.
- **Deploy URLs are `*.edgeone.run`** (not `*.edgeone.app`), and are
  **preview/preset deployments gated by a one-time `?eo_token=…&eo_time=…`**
  query pair that 302-redirects and sets `eo_token`/`eo_time` HttpOnly cookies
  (Max-Age 10800 s = 3 h). So every curl must carry the query pair AND follow
  the redirect with a cookie jar:
  `curl -L -c jar -b jar "https://<host>/<path>?eo_token=…&eo_time=…"`.
  Without it: `HTTP 401  X-EOP-MSG: eo_time missing`.
- **DNS is clean** (no hijack): `*.edgeone.run` resolves to a real EdgeOne edge
  IP `61.241.178.245` (`pages.openedge.sched.txdl1.com`); no `--resolve` needed.

## Function convention (learned from `edgeone pages init`)

- Edge functions live under **`edge-functions/`** (NOT `functions/`). Each file
  is a route by its path: `edge-functions/healthz.js` → `/healthz`. Subdirs are
  allowed; a `_lib/` subdir holds importable modules (its files are not served
  as functions). The platform documents/supports `edge-functions/[[default]].js`
  as a catch-all, but the GPROXY direct-upload package currently uses explicit
  route files because the root catch-all fell back to static assets.
- Handler is the exported **`onRequest(context)`** returning a `Response`
  (Edge Functions do NOT support `addEventListener`). `context` = `{ request,
  env, params, uuid, waitUntil }`; **env vars come from `context.env`**.
- `generate-routes` only emits a static `routes.json` (filesystem handler); it
  is NOT how functions register — the platform auto-discovers `edge-functions/`
  during the build. (It reports "No server-handler detected … pure project"
  even for projects that DO have working functions, so it is a red herring.)

## Step 1 — Auth: **OK** (see above).

## Step 2 — Trivial deploy: **LIVE** ✅

`edgeone pages deploy deploy/eopages/trivial --name gproxy-spike -t <TOK>
-e production` → project `pages-duubpxy7tneq`, host
`gproxy-spike-te2iwbpy.edgeone.run`.

```
GET /healthz  -> 200  "hello-edgeone-pages"     (edge-functions/healthz.js, onRequest)
GET /         -> 200  <static index.html>
```

## Step 3 — WASM instantiate (minimal module): **WORKS** ✅

`edge-functions/wasmtest.js` decodes a 41-byte base64-embedded wasm module
(exports `add(i32,i32)->i32`) and runs
`WebAssembly.instantiate(bytes, {}); exports.add(2,3)`:

```
GET /wasmtest -> 200  "5"
```

**EdgeOne Pages edge functions expose the `WebAssembly` global AND allow
`WebAssembly.instantiate(bytes, imports)` (runtime byte/buffer compilation)** —
the same capability tier as Netlify / Supabase / Deno, and MORE permissive than
Cloudflare (which forbids buffer instantiation and requires a static
`?module` import). So the base64-inline glue approach is the right model.

## Step 4 — Real GPROXY wasm: **LIVE** ✅ (after shrink + lazy explicit routes)

The original deno-target glue + base64-inlined 612 KB `gproxy_bg.wasm`
(`deploy/eopages/build.sh`) failed under EdgeOne Pages. A catch-all
`[[default]].js` also caused direct uploads to fall through to the static index;
even a plain `/probe` route returned `index.html` through that shape.

Isolation testing (a separate `gproxy-iso` project) showed the first failure was
not a missing `WebAssembly` global, not `init` network I/O, and not Turso/Upstash
configuration. The old module shape localized the instantiate problem:

| probe | what it does | result |
|---|---|---|
| `/probe` | plain `onRequest`, no imports | **200** `probe-plain-ok` (functions DO register) |
| `/probedecode` | import 800 KB base64 + `atob`→bytes, **no wasm** | **200** `decode-ok bytes=612854 ms=108` |
| `/probecompile` | decode + **`WebAssembly.compile(bytes)`** (no instantiate) | **200** `compile-ok compile_ms=3 exports=14` |
| `/probeinst` | decode + **`WebAssembly.instantiate(bytes, imports)`** (glue) | **TCP reset @ ~15.05 s** |
| `/probesplit` | instantiate with `__wbindgen_start()` wrapped in try/catch (never throws) | **TCP reset @ ~15.09 s** |
| `/probewasm` | full `init()` + `wasmFetch('/version')` | **TCP reset @ ~15.10 s** |

Old failure chain:
- A 41-byte module instantiates instantly (`5`). ✅
- The 612 KB module **decodes** fine (108 ms) and **compiles** fine (3 ms). ✅
- The 612 KB module **`instantiate()` is hard-killed at a fixed ~15.0 s**, with
  or without the Rust start fn, and emits **no response headers** (connection
  held open exactly 15 s, then RST, 0 bytes). ❌
- Locally in node v22 the identical bytes `instantiate` in ~0 ms with 51 stub
  imports — so the module is cheap on standard V8; the kill is EdgeOne-specific.

The fixed path changes two things:

1. The release wasm is smaller before bindgen inline output. In the local
   environment, `wasm-opt` was unavailable and `npx -p binaryen wasm-opt` failed
   with `ECONNRESET`, so the Cargo release profile alone was used.
2. The generated glue no longer instantiates wasm at top level. It exports
   `__gproxy_load()`, and the explicit route handlers call that inside
   `onRequest()` before `init()` and `wasmFetch()`.

Live verification after the fix:

| route | result |
|---|---|
| `/probe` | `200 probe-ok` with an explicit route file |
| `/loadprobe` | `200 load-ok ms=65` (`__gproxy_load()` only) |
| `/healthz` | `200 {"status":"ok"}` after cleaned redeploy |
| `/version` | `200 {"version":"2.0.0"}` after cleaned redeploy |

Conclusion: EdgeOne Pages can run GPROXY's wasm build, but only with the
optimized release profile, inline lazy loader, and explicit route files.

## Exact CLI commands (secrets redacted)

```bash
npm i -g edgeone                                   # v1.5.6
env -i PATH=$PATH HOME=$HOME EDGEONE_PAGES_API_TOKEN=<TOK> edgeone whoami   # auth OK (100016841661)

# trivial (functions + wasm work)
edgeone pages deploy deploy/eopages/trivial  --name gproxy-spike -t <TOK> -e production
#   -> https://gproxy-spike-te2iwbpy.edgeone.run?eo_token=<…>&eo_time=<…>
curl -L -c jar -b jar "https://gproxy-spike-te2iwbpy.edgeone.run/wasmtest?eo_token=<…>&eo_time=<…>"  # -> 5
curl -L      -b jar "https://gproxy-spike-te2iwbpy.edgeone.run/healthz"                              # -> hello-edgeone-pages

# real GPROXY
cargo build --lib --target wasm32-unknown-unknown --release --no-default-features --features edge
bash deploy/eopages/build.sh                 # deno-target glue + lazy base64-inline patch
edgeone pages deploy deploy/eopages/gproxy   --name gproxy-v2 -e production
edgeone pages link                           # (enter: gproxy-v2) -> .edgeone/project.json
edgeone pages env set TURSO_URL   <REDACTED> -t <TOK>     # + TURSO_TOKEN, UPSTASH_URL, UPSTASH_TOKEN,
                                                          #   GPROXY_MASTER_KEY (optional — unseals stored secrets)
edgeone pages deploy deploy/eopages/gproxy   --name gproxy-v2 -e production
curl -L -c jar -b jar "https://gproxy-v2-g1yrgdxl.edgeone.run/healthz?eo_token=<…>&eo_time=<…>"
#   -> {"status":"ok"}
curl -L -c jar -b jar "https://gproxy-v2-g1yrgdxl.edgeone.run/version?eo_token=<…>&eo_time=<…>"
#   -> {"version":"2.0.0"}
```

Old failure signature (pre-shrink wasm-instantiate paths): no HTTP status, no
headers — `curl: (56) Recv failure: Connection reset by peer` at
`time_total ≈ 15.05–15.10 s`.

## Bottom line — Can GPROXY's WASM run on EdgeOne Pages?

**YES, with constraints.** EdgeOne Pages edge functions run WebAssembly and
GPROXY's wasm build now serves real `/healthz` and `/version` routes. Step 1
(shrink + lazy inline loader) was enough, so the Pages Node Functions fallback
is not needed right now.

Remaining follow-ups:
1. Install/use native `wasm-opt` in CI or release packaging and record the final
   post-link byte size.
2. Re-test `[[default]].js` only if catch-all routing becomes necessary; explicit
   route files are the known-good shape.
3. File/confirm the exact Edge Function wasm-instantiation limit with Tencent if
   future wasm growth approaches the budget again.

## Reproduce

```bash
cd /home/linhuan/gproxy/v2
set -a && source ./.env && set +a            # EDGEONE_PAGES_API_TOKEN (+ GPROXY_TEST_* storage)
cargo build --lib --target wasm32-unknown-unknown --release --no-default-features --features edge
bash deploy/eopages/build.sh
# trivial wasm proof:
edgeone pages deploy deploy/eopages/trivial --name gproxy-spike -t "$EDGEONE_PAGES_API_TOKEN" -e production
#   then curl /wasmtest (carry the printed eo_token/eo_time + cookie jar) -> 5
# real GPROXY:
edgeone pages deploy deploy/eopages/gproxy  --name gproxy-v2 -e production
#   then curl /healthz and /version (carry printed eo_token/eo_time + cookie jar)
#   -> {"status":"ok"} and {"version":"2.0.0"}
```

**Cleanup:** three preview projects were created on the account —
`gproxy-spike` (`pages-duubpxy7tneq`, trivial + wasm probes),
`gproxy-v2` (`pages-zsrplszrfd5s`, real GPROXY + storage env vars set),
`gproxy-iso` (`pages-aiopejin…`, throwaway isolation probes). They are
preview/preset deployments (token-gated, not public). No `teo` zone / DNS / paid
resource was provisioned. Delete from the EdgeOne Pages console if desired.
No curl was faked.
