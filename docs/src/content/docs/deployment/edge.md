---
title: "Edge Wasm"
description: "Deploy the gproxy edge bundle to Cloudflare Workers, Deno Deploy, or Netlify Edge with a libSQL database and optional Upstash cache"
---

`crates/gproxy-host-edge` compiles the same application core as the native
binary to `wasm32-unknown-unknown` for fetch-based runtimes. A short
TypeScript entry per platform reads the bindings, constructs an
`EdgeConfig`, calls `start()` once per isolate, and hands every non-static
request to `EdgeHost.fetch(request, clientIp)`. The admin API, the portal
API, aggregated `/v1/...` paths, and named-provider paths behave as on
native; the differences are listed under Limits. The platform sources live
in `deploy/cloudflare`, `deploy/deno`, and `deploy/netlify`.

## Bindings

| Binding | Required | Purpose |
| --- | --- | --- |
| `GPROXY_LIBSQL_URL` | yes | Absolute `https://` URL of the libSQL database. The store speaks Hrana over HTTP; a `libsql://` URL is rejected. |
| `GPROXY_LIBSQL_AUTH_TOKEN` | yes | Database auth token. |
| `GPROXY_MASTER_KEY` | no | Standard base64, 32 bytes. Seals credentials and user keys with AES-256-GCM; unset means plaintext. |
| `GPROXY_MASTER_KEY_NEXT` | no | Rotation target; an empty value rotates back to plaintext. |
| `GPROXY_MASTER_KEY_ROTATE` | no | `1`, `true`, `yes`, or `on` arms the rotation for one deploy. |
| `UPSTASH_URL`, `UPSTASH_TOKEN` | no | Upstash REST cache. The shipped Cloudflare, Deno and Netlify entries pass both bindings to `EdgeConfig`; set both or neither. |

Store the values as platform secrets. Nothing is read from a `.env` file on
edge, and the `GPROXY_ADMIN_*` first-run variables are native-only.

Turso is the usual libSQL provider: create a database and a token, and use
the database's HTTPS URL (`https://<db>-<org>.turso.io`). On the first
request the store creates its schema and seeds the global price catalog.
The cache that holds quotas, rate limits, refresh leases, and admission
state is a table in the same database, so every isolate sees the same
state. See [Storage & Cache Backends](/reference/database/).

## Static Layer and Rust

| Path | Served by |
| --- | --- |
| `/`, `/admin`, `/admin/**` (except `/admin/api/**`), `/portal`, `/portal/` | Static `index.html` (console SPA), `GET`/`HEAD` only |
| `/assets/**`, `/favicon.svg` | Static console assets |
| `/admin/api/**`, `/portal/api/**` | Rust: admin and portal dispatch |
| Everything else | Rust: gateway ingress (`/v1/...`, Claude and Gemini native paths, named-provider paths, WebSocket upgrades) |

Cloudflare's `wrangler.toml` sets `run_worker_first = true`, so the Worker
sees every request and forwards static ones to the `ASSETS` binding,
rewriting `/admin/*` to `/`. Deno's `main.ts` reads the same files from
`public/`. Netlify keeps the static paths on its CDN through `excludedPath`
in `netlify.toml` and rewrites `/admin/*` to `/`. The client IP handed to
Rust comes from `cf-connecting-ip`, `remoteAddr.hostname`, or `Context.ip`
respectively. Request bodies are limited to 100 MiB, as on native.

## WebSockets

| Platform | Upgrade | Mechanism |
| --- | --- | --- |
| Cloudflare Workers | yes | `WebSocketPair`; the pump is kept alive with `ExecutionContext.waitUntil` |
| Deno Deploy | yes | `Deno.upgradeWebSocket`; `main.ts` retains the continuation until it settles |
| Netlify Edge | no | No upgrade API. Rust answers `501` with `websocket upgrades are unavailable in this fetch runtime` |

## Prebuilt Bundles

Every release publishes `gproxy-edge-cloudflare.zip`, `gproxy-edge-deno.zip`,
`gproxy-edge-netlify.zip`, and the raw `gproxy-edge.wasm`, each with a
`.sha256`, plus `gproxy-edge.provenance.json`. A zip unpacks to
`<platform>/` with the entry file, its config, `pkg/` (wasm and
wasm-bindgen glue), and `public/` (the console build). GitHub's
`releases/latest` excludes prereleases, so while v3 is in alpha download
from the versioned release page; see [Downloads](/getting-started/downloads/).

Both `wrangler.toml` and `netlify.toml` declare a
`[build] command = "pnpm run build"` that compiles from source with
`wasm-pack`. When you deploy a prebuilt bundle, remove that section so the
platform does not attempt a source build.

### Cloudflare Workers

```sh
cd cloudflare
pnpm install
pnpm exec wrangler secret put GPROXY_LIBSQL_URL
pnpm exec wrangler secret put GPROXY_LIBSQL_AUTH_TOKEN
pnpm exec wrangler deploy
```

`wrangler.toml` names `src/index.ts` as the Worker, `./public` as the assets
directory bound as `ASSETS`, and a compatibility date of `2026-08-26`.
`pnpm run dev` runs `wrangler dev` locally.

### Deno Deploy

The bundle is `main.ts`, `deno.json`, `pkg/`, and `public/`. Point the
project at `main.ts` and set the bindings as project environment variables.
Locally, `deno task start` runs
`deno run --allow-net --allow-env=<the five GPROXY_* names> --allow-read=./pkg,./public main.ts`.

### Netlify Edge

`netlify.toml` publishes `public/` and registers the `gproxy` Edge Function
on `/*` minus the static paths. Set `GPROXY_LIBSQL_URL` and
`GPROXY_LIBSQL_AUTH_TOKEN` as sensitive site variables, then:

```sh
cd netlify
pnpm install
pnpm run deploy   # netlify deploy --prod
```

## Building Bundles Yourself

You need Rust with the `wasm32-unknown-unknown` target, `wasm-pack`, Node.js
LTS with pnpm, and Deno for the Deno bundle.

```sh
cd deploy/cloudflare && pnpm install && pnpm run build && pnpm run check
cd deploy/netlify && pnpm install && pnpm run build && pnpm run check
cd deploy/deno && deno task build && deno task check
```

`build:wasm` runs `wasm-pack build ../../crates/gproxy-host-edge --release`
with `--target bundler` for Cloudflare and `--target web` for Deno and
Netlify, writing `pkg/`. `build:assets` builds the console and copies
`console/dist` to `public/`. Both directories are gitignored.
`scripts/package-edge-release.sh` produces all three zips from a single
`cargo build`; it needs a prebuilt `console/dist` and a `wasm-bindgen` CLI
matching `Cargo.lock`. See [Building & Releases](/deployment/release-build/).

## First Boot

Open `https://<your-deployment>/admin`. On an empty store
`GET /admin/api/session` reports `setup_required: true` and the console
shows the setup form; `POST /admin/api/setup` creates the first
administrator and signs you in. From there the workflow is the native one:
add a provider, paste or log in a credential, create a route, and issue a
user key; see [Quick Start](/getting-started/quick-start/). `/portal` works
the same way for users.

## Limits

| Native feature | On edge |
| --- | --- |
| Claude Web channel | Not compiled into the wasm build |
| Provider or credential proxy override | The request fails with `configured upstream proxy is unavailable in the fetch runtime` |
| TLS/HTTP2 fingerprint override | The request fails with `configured TLS/HTTP2 fingerprint is unavailable in the fetch runtime` |
| Default outbound proxy (`GPROXY_UPSTREAM_PROXY_URL`) | None; egress is the platform's `fetch` |
| Connectivity probe | `400` with `connectivity testing is unavailable on edge` |
| Tokenizer vocabulary download, Hugging Face token | `403`; the built-in tokenizer ladder still counts tokens |
| Self-update, autostart, announcements | Absent; these are native host routes |
| SQLite, PostgreSQL, MySQL, Redis | Not available; libSQL only, Upstash optional |
| WebSocket on Netlify | `501` |

Each isolate starts its own host and loads its own snapshot from libSQL;
isolates share no memory, and the libSQL cache table (or Upstash) is what
keeps quotas and rate limits consistent between them.
