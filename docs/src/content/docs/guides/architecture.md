---
title: Architecture
description: GPROXY workspace layering and request execution model.
---

## Workspace structure

| Path | Responsibility |
|---|---|
| `apps/gproxy` | Executable service entry (Axum + embedded frontend assets) |
| `crates/gproxy-core` | AppState, route orchestration, auth, request execution |
| `sdk/gproxy-provider` | Channel implementations, retry, OAuth, dispatch, tokenizer |
| `crates/gproxy-middleware` | Protocol conversion middleware, usage extraction |
| `sdk/gproxy-protocol` | OpenAI/Claude/Gemini types and conversion models |
| `crates/gproxy-storage` | SeaORM storage layer, query models, async write queue |
| `crates/gproxy-admin` | Admin/user domain services |

## Startup phase

Main startup sequence:

1. Read `gproxy.toml` and apply CLI/ENV overrides.
2. Connect to DB and auto-sync schema.
3. Initialize provider registry, credential pool, and credential statuses.
4. Ensure admin user (`id=0`) and admin key exist.

## Request phase

Core request chain:

1. Auth: parse and validate `x-api-key` (or compatible headers).
2. Routing: resolve target provider by scoped/unscoped routes + dispatch.
3. Credential selection: select available credential and do retry/failover.
4. Protocol conversion: convert or pass through by provider protocol rules.
5. Recording: persist upstream/downstream requests and usage.

## Credential health status

- `healthy`: available.
- `partial`: partially available (typically model-level cooldown).
- `dead`: unavailable (excluded from scheduling).

Default cooldown behavior:

- rate limit: around `60s`
- transient failure: around `15s`

This mechanism reduces the impact of single-credential instability on overall success rate.
