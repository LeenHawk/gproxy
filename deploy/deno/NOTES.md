# Deno Deploy — WASM Deployment Notes

**Date:** 2026-06-04
**Product:** Deno Deploy new platform.
**Question:** Can the existing `deploy/deno/main.ts` entry be deployed and
verified?

## STATUS: `GPROXY_WASM_LIVE`

GPROXY v2's wasm edge build is live on Deno Deploy:

```text
Production URL: https://gproxy-deno.leenhawk20.deno.net
GET /healthz  -> 200 {"status":"ok"}    time=0.942s
GET /version  -> 200 {"version":"2.0.0"} time=0.930s
```

The app is:

```text
org: leenhawk20
app: gproxy-deno
```

Environment variables set on the app:

```text
TURSO_URL
TURSO_TOKEN
UPSTASH_URL
UPSTASH_TOKEN
GPROXY_MASTER_KEY   (optional — unseals encrypted stored secrets)
```

## Important CLI Notes

The old Deploy Classic path is blocked for new projects. `deployctl` accepted
the token but tried to create a missing Classic project and failed:

```text
Project 'gproxy-deno' not found. Creating...
error: APIError: New project creation is disabled. Deno Deploy Classic is being
sunset on July 20, 2026. Please migrate to the new Deno Deploy platform.
```

Use the new Deno Deploy CLI instead:

```bash
deno run -A https://jsr.io/@deno/deploy/0.0.99/main.ts --token "$DENO_DEPLOY_TOKEN" --prod /tmp/gproxy-deno-upload
```

In this environment, the shorthand `deno deploy` initially failed to resolve
`jsr:@deno/deploy` because Cloudflare challenged the package metadata request:

```text
error: Import 'https://jsr.io/@deno/deploy/meta.json' failed: 403 Forbidden
```

Pinning the direct module URL `https://jsr.io/@deno/deploy/0.0.99/main.ts`
worked and cached the CLI.

## Build And Deploy

Run from the crate root:

```bash
set -a && source ./.env && set +a
bash deploy/deno/build.sh
```

The script:

1. Builds `target/wasm32-unknown-unknown/release/gproxy.wasm`.
2. Runs `wasm-bindgen --target deno --out-dir pkg`.
3. Patches the generated loader to call `globalThis.fetch(wasmUrl)` so the Rust
   export named `fetch` does not shadow Deno's global `fetch`.
4. Creates `/tmp/gproxy-deno-upload` with root `main.ts` and `pkg/`.
5. Deploys that compact upload root to `leenhawk20/gproxy-deno`.

The compact upload root matters because the new Deno Deploy app stores build
configuration. The verified shape has root `main.ts` importing `./pkg/gproxy.js`;
deploying directly from the repo root with `deploy/deno/main.ts` did not update
the app entrypoint and produced a failed revision.
