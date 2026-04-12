---
title: Architecture
description: A tour of the gproxy workspace — apps, crates, and SDK layers — and how a request flows through the system.
---

gproxy is a Cargo workspace organized into three layers:

1. **Apps** — runnable binaries (`gproxy`, `gproxy-recorder`).
2. **Crates** — the server-side pieces that the main app composes (`gproxy-core`,
   `gproxy-storage`, `gproxy-api`, `gproxy-server`).
3. **SDK** — framework-agnostic libraries that can be reused outside the server
   (`gproxy-protocol`, `gproxy-routing`, `gproxy-provider`, `gproxy-sdk`).

## Workspace layout

```text
gproxy/
├── apps/
│   ├── gproxy/              # The main binary (HTTP server + console host)
│   └── gproxy-recorder/     # Upstream traffic recorder (dev/debugging tool)
├── crates/
│   ├── gproxy-core/         # Config, identity, policy, quota, routing types
│   ├── gproxy-storage/      # SeaORM storage + encryption + migrations
│   ├── gproxy-api/          # Admin + user HTTP API, auth, login, CORS
│   └── gproxy-server/       # The Axum server wiring it all together
├── sdk/
│   ├── gproxy-protocol/     # OpenAI/Claude/Gemini wire types + transforms
│   ├── gproxy-routing/      # Route classification, permission/limit matching
│   ├── gproxy-provider/     # Channel trait, ProviderStore, GproxyEngine
│   └── gproxy-sdk/          # Umbrella crate re-exporting the three above
├── frontend/console/        # React console, embedded into the binary at build time
└── docs/                    # This documentation site
```

## Request lifecycle

At a high level, an incoming LLM request goes through these stages:

```text
                ┌─────────────────────────────────────────────┐
HTTP request ──►│  gproxy-server (Axum)                       │
                │    ├── auth: API key → user identity        │
                │    ├── protocol classification (OpenAI/…)   │
                │    └── handler dispatch                     │
                └───────────────┬─────────────────────────────┘
                                │
                                ▼
                ┌─────────────────────────────────────────────┐
                │  gproxy-routing                             │
                │    permission → rewrite → alias → execute   │
                └───────────────┬─────────────────────────────┘
                                │ resolved (provider, model)
                                ▼
                ┌─────────────────────────────────────────────┐
                │  gproxy-provider :: GproxyEngine            │
                │    ├── channel.prepare_request(...)         │
                │    ├── HTTP call to upstream                │
                │    ├── retries + health updates             │
                │    └── usage accounting                     │
                └───────────────┬─────────────────────────────┘
                                │
                                ▼
                          upstream LLM API
```

1. **Auth.** The request carries an API key (`Authorization: Bearer …`). The
   API layer resolves it to a `User`, checks that the user and key are
   enabled, and attaches the identity.
2. **Classification.** The router figures out the protocol (OpenAI Chat,
   OpenAI Responses, Claude, Gemini) and the route kind (`model_list`,
   `model_get`, a chat invocation, etc.).
3. **Model resolution.** `permission → rewrite → alias → execute` is the
   single canonical order (see *Guides → Models & Aliases*). Aliases,
   permission glob patterns, and channel-level rewrite rules are applied
   here.
4. **Dispatch.** For `*-only` presets, `model_list` and `model_get` are
   served **locally** from the `models` table, never touching upstream. For
   `*-like` / pass-through presets, upstream is called and its response is
   merged with the local `models` table so admins still see what they
   registered.
5. **Execution.** `GproxyEngine::execute` asks the resolved channel to
   prepare the upstream request, makes the call, handles retries, and
   updates per-credential health state.
6. **Accounting.** Usage is captured through a sink, batched, and written
   to storage by background workers.

## Background workers

`apps/gproxy/src/workers/mod.rs` wires up a set of long-running tasks:

| Worker | Purpose |
| --- | --- |
| `UsageSink` | Drains usage messages and writes them to storage in batches. |
| `HealthBroadcaster` | Debounces per-credential health state changes. |
| `QuotaReconciler` | Periodically reconciles users' `cost_used` against recorded usage. |
| `RateLimitGC` | Garbage-collects expired rate-limit counters. |

All of them participate in graceful shutdown — see
[Graceful Shutdown](/reference/graceful-shutdown/).

## Storage

`gproxy-storage` is built on **SeaORM + SQLx** and supports **SQLite**,
**PostgreSQL**, and **MySQL**. When `DATABASE_SECRET_KEY` is set, credential
material (provider credentials, user passwords, API keys) is encrypted at
rest with **XChaCha20-Poly1305**.

The embedded console, the admin HTTP API, and the TOML seed config all write
into the same schema — the TOML file is only consulted on first-time
initialization when the database is empty. See
[TOML Config](/reference/toml-config/) for how the seed is loaded.

## SDK boundary

The `sdk/` crates intentionally have no dependency on the database, the HTTP
server, or Axum. That separation is what lets you take `gproxy-sdk` and embed
the provider engine into an entirely different application. See the
[Rust SDK](/reference/sdk/) reference for details.
