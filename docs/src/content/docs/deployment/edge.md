---
title: Edge Wasm Deployment
description: Deploy prebuilt gproxy v2 WebAssembly bundles to supported edge platforms.
---

The edge runtime is the same single Rust crate compiled as a
`wasm32-unknown-unknown` library with `--no-default-features --features edge`.
Platform entry code loads wasm-bindgen glue, calls Rust `init(...)` to build
`AppState`, and forwards each request to the wasm `fetch` path.

Do not rely on edge platforms compiling Rust from source. Use prebuilt bundles
from a release or from the `deploy` branch, or build the bundle in CI and upload
the generated output.

## Runtime Services

Edge runtimes do not connect to local SQLite, PostgreSQL, MySQL, or Redis. v2
edge uses HTTP-accessible services:

| Variable | Required | Purpose |
| --- | --- | --- |
| `TURSO_URL` | Yes | libSQL/Turso control-plane database. |
| `TURSO_TOKEN` | Yes | Turso access token. |
| `UPSTASH_URL` | No | Upstash Redis cache; falls back to libSQL KV when absent. |
| `UPSTASH_TOKEN` | No | Upstash token. |
| `GPROXY_MASTER_KEY` | No | Standard base64 32-byte key for sealed secrets. |

Set these as platform secrets or environment variables. Do not bake them into a
bundle.

## Prebuilt Bundles

The release workflow publishes:

| Artifact | Target |
| --- | --- |
| `gproxy-edge-cloudflare.zip` | Cloudflare Workers. |
| `gproxy-edge-netlify.zip` | Netlify Edge Functions. |
| `gproxy-edge-supabase.zip` | Supabase Edge Functions. |
| `gproxy-edge-deno.zip` | Deno Deploy compact upload root. |
| `gproxy-edge-eopages.zip` | Tencent EdgeOne Pages. |
| `gproxy-edge-appwrite-deno.zip` | Appwrite Functions on `deno-2.0`. |
| `gproxy.wasm` | Raw wasm artifact for inspection or custom packaging. |

On published releases the workflow also refreshes the orphan `deploy` branch
with ready-to-deploy artifacts only: wasm, glue, platform entry files, and
config. That branch contains no source build workflow.

## Local Bundle Build

Use local builds for validation or temporary artifacts:

```bash
cargo build --lib --target wasm32-unknown-unknown --release \
  --no-default-features --features edge
```

`wasm-bindgen-cli` must match the `wasm-bindgen` crate version in `Cargo.lock`.
The current workflow installs `0.2.123`.

Generate platform bundles:

```bash
bash deploy/cloudflare/build.sh
bash deploy/netlify/build.sh
bash deploy/supabase/build.sh
bash deploy/eopages/build.sh
bash deploy/appwrite-deno/build.sh
```

`deploy/deno/build.sh` is different: it builds and deploys through Deno's Deploy
CLI module, so the release workflow generates the Deno bundle inline instead of
calling that script.

## Platform Shapes

| Platform group | Bundle shape |
| --- | --- |
| Cloudflare Workers | `wasm-bindgen --target web`; `.wasm` packaged as a static `WebAssembly.Module`. |
| Netlify, Supabase, EdgeOne, Appwrite Deno | `wasm-bindgen --target deno`; wasm base64-inlined for runtime instantiate. |
| Deno Deploy | `main.ts` plus generated `pkg/` directory. |

Cloudflare does not allow arbitrary runtime wasm compilation from byte buffers,
so it uses the static module path. The Deno-family targets can instantiate from
bytes and use self-contained bundles to avoid losing sibling `.wasm` files
during platform packaging.

## Deploy Checklist

1. Create a Turso database and token.
2. Decide whether to use Upstash or the libSQL KV fallback for cache.
3. Generate and store `GPROXY_MASTER_KEY` if secrets are sealed.
4. Upload the platform bundle.
5. Configure secrets.
6. Route all gateway, admin, user, and ops paths to the worker/function.
7. Serve Console assets same-origin if you need the web UI.

## Platform Notes

Cloudflare Workers uses `deploy/cloudflare/wrangler.toml` with a compiled wasm
rule. Run `wrangler deploy` from `deploy/cloudflare` after setting secrets.

Netlify uses `deploy/netlify/netlify.toml` and the `edge-functions/` entry. Set
site environment variables with `netlify env:set`, then run
`netlify deploy --prod`.

Supabase uses `deploy/supabase/functions/gproxy` and should be deployed with
`supabase functions deploy gproxy --no-verify-jwt`. Avoid the API upload path
when it drops sibling wasm files.

EdgeOne Pages uses `deploy/eopages/gproxy` and needs a recent `edgeone` CLI.
The generated catch-all edge function receives dynamic paths while `/` can still
be served as platform static content.

Deno Deploy uses a compact root containing `main.ts`, `pkg/`, and `deno.json`.
The current path uses the new Deno Deploy CLI module rather than old Deploy
Classic `deployctl`.

Appwrite Functions run the prebuilt wasm through the `deno-2.0` runtime. Do not
use Appwrite's Rust runtime for this bundle.

## Edge Limitations

The edge runtime shares the same routing engine, transform pipeline, admin/user
dispatcher, and protocol logic where possible, but a few native-only APIs return
`501 not_implemented`:

- `/admin/update/*`
- `/admin/login-flows/cookie`
- `/admin/credentials/{id}/usage`

Ops endpoints (`/healthz`, `/version`, `/metrics`) are admin-gated on edge just
as they are on native.
