---
title: Edge Wasm Deployment
description: Deploy prebuilt GPROXY v2 WebAssembly bundles to supported edge platforms.
---

The edge runtime is the same single Rust crate compiled as a
`wasm32-unknown-unknown` library with `--no-default-features --features edge`.
Platform entry code loads wasm-bindgen glue, calls Rust `init(...)` to build
`AppState`, and forwards each request to the wasm `fetch` path.

Do not rely on edge platforms compiling Rust from source. Use prebuilt bundles
from a release or from the `deploy` branch, or build the bundle in CI and upload
the generated output.

## One-click Deploy

[![Deploy to Cloudflare](https://deploy.workers.cloudflare.com/button)](https://deploy.workers.cloudflare.com/?url=https://github.com/LeenHawk/gproxy/tree/deploy/cloudflare)
[![Deploy to Netlify](https://www.netlify.com/img/deploy/button.svg)](https://app.netlify.com/start/deploy?repository=https://github.com/LeenHawk/gproxy&branch=deploy&create_from_path=netlify)

These buttons use the prebuilt `deploy` branch artifacts. Configure the runtime
services below as platform secrets.

The Cloudflare template declares required Turso secrets in
`deploy/cloudflare/.dev.vars.example`, and the Netlify template declares them in
`deploy/netlify/netlify.toml`. Both one-click flows prompt for `TURSO_URL` and
`TURSO_TOKEN` before the first deploy. Use Turso's HTTP URL
(`https://<db>.turso.io`), not the `libsql://` URL, because edge runtimes call
Hrana over `fetch`. They also prompt for `GPROXY_ADMIN_USER` and
`GPROXY_ADMIN_PASSWORD`; the password must be non-blank and is used for the
first Console login. Add optional `UPSTASH_URL`, `UPSTASH_TOKEN`, and
`GPROXY_MASTER_KEY` secrets later if the deployment needs Upstash cache or
sealed stored secrets.

Cloudflare Workers, Netlify Edge, and Deno Deploy bundle the
React Console into the same deployment and redirect `/` to `/console/`.

## Runtime Services

Edge runtimes do not connect to local SQLite, PostgreSQL, MySQL, or Redis. v2
edge uses HTTP-accessible services:

| Variable | Required | Purpose |
| --- | --- | --- |
| `TURSO_URL` | Yes | libSQL/Turso HTTP URL, for example `https://<db>.turso.io`. |
| `TURSO_TOKEN` | Yes | Turso access token. |
| `GPROXY_ADMIN_USER` | Yes | Admin username for the Console login. |
| `GPROXY_ADMIN_PASSWORD` | Yes | Non-blank admin password for the Console login. |
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
| `gproxy-edge-deno.zip` | Deno Deploy compact upload root. |
| `gproxy.wasm` | Raw wasm artifact for inspection or custom packaging. |

On published releases the workflow also refreshes the orphan `deploy` branch
with ready-to-deploy artifacts only: wasm, glue, platform entry files, and
config. That branch contains no source build workflow.

## Local Bundle Build

Use local builds for validation or temporary artifacts:

```bash
cargo rustc --lib --crate-type cdylib --target wasm32-unknown-unknown --release \
  --no-default-features --features edge
```

`wasm-bindgen-cli` must match the `wasm-bindgen` crate version in `Cargo.lock`.
The current workflow installs `0.2.126`.

Generate platform bundles:

```bash
bash deploy/cloudflare/build.sh
bash deploy/netlify/build.sh
```

`deploy/deno/build.sh` is different: it builds and deploys through Deno's Deploy
CLI module, so the release workflow generates the Deno bundle inline instead of
calling that script.

## Platform Shapes

| Platform group | Bundle shape |
| --- | --- |
| Cloudflare Workers | `wasm-bindgen --target web`; `.wasm` packaged as a static `WebAssembly.Module`. |
| Netlify | `wasm-bindgen --target deno`; wasm base64-inlined for runtime instantiate. |
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
5. Configure secrets, including the admin username and password.
6. Route all gateway, admin, user, and ops paths to the worker/function.
7. Serve Console assets same-origin if you need the web UI and the platform
   bundle does not already include a site-root Console.

## Platform Notes

Cloudflare Workers uses `deploy/cloudflare/wrangler.toml` with a compiled wasm
rule and a Worker static assets binding for `/console`. The one-click deploy
flow asks for the required Turso and admin secrets. For CLI deploys, set
`TURSO_URL`, `TURSO_TOKEN`, `GPROXY_ADMIN_USER`, and
`GPROXY_ADMIN_PASSWORD` with `wrangler secret put`, then run `wrangler deploy`
from `deploy/cloudflare`.

Netlify uses `deploy/netlify/netlify.toml` and the `edge-functions/` entry. Set
site environment variables with `netlify env:set`, then run `netlify deploy
--prod`. The one-click deploy flow asks for the required Turso environment
variables and admin credentials through `[template.environment]`. The publish
directory includes the Console SPA; `/console/*` is excluded from the edge
function so Netlify can serve static files and the SPA fallback.

Deno Deploy uses a compact root containing `main.ts`, `pkg/`, and `deno.json`.
The current path uses the new Deno Deploy CLI module rather than old Deploy
Classic `deployctl`. The upload root also includes the Console build, and
`main.ts` serves `/console/*` before forwarding API traffic to wasm.

## Edge Limitations

The edge runtime shares the same routing engine, transform pipeline, admin/user
dispatcher, and protocol logic where possible, but a few native-only APIs return
`501 not_implemented`:

- `/admin/update/*`
- `/admin/login-flows/cookie`

Live credential usage and rate-limit reset-credit operations use the edge fetch
transport. Upstream SSE is relayed as a real streaming `ReadableStream` on all
three supported edge targets, including per-frame protocol conversion.

Ops endpoints (`/healthz`, `/version`, `/metrics`) are admin-gated on edge just
as they are on native.
