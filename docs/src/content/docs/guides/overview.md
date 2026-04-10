---
title: Overview
description: Core positioning, capability boundaries, and engineering advantages of GPROXY.
---

## What is GPROXY

`GPROXY` is a Rust-based multi-channel LLM proxy that exposes different upstreams (OpenAI, Anthropic, Google, and custom channels) through a unified API gateway.

## Core capabilities

- Multi-channel and multi-credential-pool management with health states, retries, and failover.
- Compatible with OpenAI / Claude / Gemini-style request formats.
- Supports operation families like `ModelList / ModelGet / Generate / Stream / CountToken / Embedding / Compact`.
- Built-in Admin Web console for user and key management.
- Request and usage records for auditing and operations analytics.

## Multi-database support

The storage layer is based on SeaORM, with these drivers enabled:

- `sqlx-sqlite`
- `sqlx-mysql`
- `sqlx-postgres`

You can switch database backend via `global.dsn` without changing business logic code.

## Built-in channels and custom channels

Built-in native channels (12):

- `openai`
- `claude`
- `aistudio`
- `vertexexpress`
- `vertex`
- `geminicli`
- `claudecode`
- `codex`
- `antigravity`
- `nvidia`
- `deepseek`
- `groq`

Beyond built-in channels, `ChannelId::Custom` lets you define arbitrary channels (`id` + `base_url` + `credentials` + optional `dispatch`).

## Protocol conversion power

`dispatch` is the core mechanism in GPROXY. Every channel can define conversion behavior from source protocol to target protocol.

- Rule types: `Passthrough / TransformTo / Local / Unsupported`
- Cross-protocol conversion: OpenAI / OpenAI Chat Completion / Claude / Gemini / GeminiNDJson
- Routes by operation family instead of simple URL matching

This makes "upstream only supports protocol A natively, while you expose B/C externally" a configurable capability.

## Single-binary deployment advantages

Admin frontend assets are embedded into the backend binary via `rust-embed`:

- No extra Node.js runtime required in production.
- No extra Nginx/static hosting for admin UI.
- Delivery can be reduced to one binary plus config.
- Simpler deployment in edge or minimal environments.

### Delivery shape

- Backend entry: `apps/gproxy/src/main.rs`
- Admin UI embedding: `apps/gproxy/src/web.rs` (`frontend/dist`)
- One process can serve:
  - Admin UI (`/`)
  - Static assets (`/assets/*`)
  - API routes (`/admin/*`, `/user/*`, `/v1*`)

### Direct ops benefits

- Fewer components: no separate frontend/backend deployment.
- Better version consistency: frontend and backend ship together.
- Faster rollback: rollback a single binary.
- Lower environment requirements: friendlier to minimal hosts and private networks.
