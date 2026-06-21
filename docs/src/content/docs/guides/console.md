---
title: Console
description: Use the v2 React console to manage providers, routes, rules, identity, settings, usage, logs, and portal workflows.
---

The v2 console is a React app in `console/`. The native binary serves its built
output from `/console`, and the Vite dev server proxies backend routes during
local development.

The console talks to the same admin and user APIs that the edge runtime uses:

| Surface | Purpose |
| --- | --- |
| `/admin/*` | Admin control plane, observability, settings, update, and operations. |
| `/user/*` | User portal account, key, limit, audit, and usage views. |
| `/healthz`, `/version`, `/metrics` | Admin-gated ops endpoints. |
| `/v1/*`, `/v1beta/*`, `/{provider}/v1/*` | Gateway traffic, authenticated by user API key. |

## Local Development

Run the backend with insecure cookies only for local HTTP:

```bash
GPROXY_INSECURE_COOKIES=1 cargo run --features full
```

Then run the console:

```bash
cd console
pnpm dev
```

`console/vite.config.ts` proxies `/admin`, `/healthz`, `/version`, and
`/metrics` to `http://127.0.0.1:8787` and rewrites origin for CSRF checks. The
production native binary serves the built console and `/user/*` portal APIs
from the same origin.

## Main Admin Areas

| Area | What it manages |
| --- | --- |
| Providers | Provider records, credentials, TLS presets, provider models, upstream model pull, routing rules, provider rule-set attachments. |
| Routes | Aggregated route names, aliases, route members, strategies, and route settings. |
| Rules | Reusable rule sets and individual process rules. |
| Users | Orgs, teams, users, keys, permissions, rate limits, and quotas. |
| Usage | Usage rows, rollups, downstream/upstream request logs, audit logs, credential statuses. |
| Settings | Instance settings, proxy, logging, usage, tokenizer download, retention, update channel. |
| Update | Native self-update state where supported. |

## Build and Embed

For production native builds:

```bash
cd console
pnpm build
```

The build runs TypeScript, Vite, and `scripts/sync-to-embed.mjs`, which copies
`console/dist/` into `assets/console/` for `rust-embed`. If that step has not
run, the backend still compiles but only serves the placeholder embed directory.

## Configuration Philosophy

The console should carry provider-specific policy as presets wherever possible.
For transform behavior, that means wizards and templates should generate generic
or existing rule config. The backend process engine remains permissive and
operation-oriented.
