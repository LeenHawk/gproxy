---
title: Architecture
description: Workspace layout, startup sequence, request lifecycle, and credential health.
---

## Workspace structure

| Path | Crate | Responsibility |
|---|---|---|
| `apps/gproxy` | `gproxy` | Binary entry point. Axum server, embedded console via `rust-embed`, CLI arg parsing, background worker orchestration. |
| `sdk/gproxy-protocol` | `gproxy-protocol` | OpenAI, Claude, and Gemini request/response type definitions. Cross-protocol transform logic between all six protocol kinds. |
| `sdk/gproxy-provider` | `gproxy-provider` | `Channel` trait and all 14 built-in channel implementations. Dispatch tables, credential retry loop, billing/pricing tables, health tracking, OAuth flows. |
| `sdk/gproxy-routing` | `gproxy-routing` | Route classification from HTTP path to `(OperationFamily, ProtocolKind)`. Model alias resolution, permission checks, rate limit rules, model name extraction and sanitization. |
| `sdk/gproxy-sdk` | `gproxy-sdk` | Top-level SDK crate that re-exports `gproxy-protocol`, `gproxy-provider`, and `gproxy-routing`. |
| `crates/gproxy-core` | `gproxy-core` | Domain services: Config, Identity (user/key management), Policy (permissions), Quota (token budgets), Routing (provider resolution), File (file API proxy). |
| `crates/gproxy-server` | `gproxy-server` | `AppState` builder, global config, middleware stack, session management, price tier definitions. |
| `crates/gproxy-api` | `gproxy-api` | HTTP handlers for admin, user, and provider routes. Bootstrap logic (seed from TOML, seed defaults, reload from DB). Login and auth endpoints. |
| `crates/gproxy-storage` | `gproxy-storage` | SeaORM entity definitions, repository implementations, schema sync, async write sink (`StorageWriteEvent` channel for non-blocking persistence). |
| `frontend/console` | `@gproxy/console` | React admin console. Vite + Tailwind. Built output is embedded into the binary at compile time. |

## Startup sequence

When you run `./gproxy`, the following happens in order:

1. **Parse CLI args.** Host, port, DSN, admin credentials, config path, proxy, spoof emulation. All accept environment variable overrides.

2. **Resolve DSN.** If no `--dsn` is given, defaults to `sqlite://./data/gproxy.db?mode=rwc`. If the database already has persisted global settings pointing at a different DSN, GPROXY reconnects to that database instead.

3. **Connect DB + sync schema.** Opens the database connection via SeaORM. Runs schema sync (creates tables, adds new columns for migrations). No separate migration CLI -- it's automatic.

4. **Decide: reload or seed.** If the database already has global settings, GPROXY loads all state from DB (providers, users, keys, models). If the database is empty, it looks for `gproxy.toml`:
   - **TOML exists:** parse it, seed providers/credentials/users/models into the database.
   - **No TOML:** create minimal defaults (empty provider list).

5. **Reconcile bootstrap admin.** Ensure user `id=0` (the admin) exists. If admin credentials were given via CLI flags, apply them. If not, and this is first run, generate a random password and API key, print them to stdout.

6. **Start background workers:**
   - **Usage sink** -- reads from an mpsc channel and batch-writes usage records to the database.
   - **Quota reconciler** -- periodically syncs quota balances from accumulated usage.
   - **Rate limit GC** -- cleans up expired rate limit windows.
   - **Health broadcaster** -- subscribes to credential health changes and pushes updates to connected console WebSocket clients.

7. **Bind Axum server.** Merges the API router (admin/user/provider routes) with the console router (embedded SPA). Starts listening on `host:port` with graceful shutdown on SIGTERM/Ctrl+C.

## Request lifecycle

A request to GPROXY follows this path:

1. **Auth.** Extract the API key from `Authorization: Bearer ...` or `x-api-key` header. Look up the identity (user + key). Check if the key is enabled, the user is active, and hasn't exceeded quota.

2. **Route classification.** Parse the URL path into `(OperationFamily, ProtocolKind)`. Paths can be:
   - **Unscoped:** `/v1/chat/completions` -- GPROXY resolves the provider from the model name in the request body.
   - **Provider-scoped:** `/my-provider/v1/chat/completions` -- the first path segment names the provider directly.

3. **Dispatch table lookup.** Given the resolved provider's channel, look up the `(operation, protocol)` pair in the dispatch table. This returns one of: `Passthrough`, `TransformTo { destination }`, `Local`, or `Unsupported`.

4. **Protocol transform (request side).** If the dispatch says `TransformTo`, convert the request body from the source protocol to the destination protocol. For example, OpenAI Chat Completions request body becomes a Claude Messages request body.

5. **Suffix processing.** Strip model suffixes (e.g. `-thinking`, `-fast`, `-1m`) and apply their effects (enable extended thinking, set speed hints, adjust context window parameters). Suffixes are resolved from both protocol-level groups and channel-specific groups.

6. **`finalize_request`.** Channel-specific body normalization that should be visible to routing and cache-affinity logic. Runs after protocol transform but before credential selection.

7. **Credential selection.** Pick a healthy credential from the provider's pool. Two selection modes:
   - **Round-robin** (default) -- rotate through healthy credentials.
   - **Cache affinity** -- hash the request content to pin similar prompts to the same credential, maximizing upstream prompt cache hits.
   Rate limit windows are checked here. If a credential's rate limit is exhausted, skip to the next.

8. **`prepare_request`.** Build the actual HTTP request: set the URL, inject auth headers, apply channel-specific transport wrapping (API key headers, OAuth bearer tokens, request IDs).

9. **HTTP send.** Send the request via the appropriate HTTP client. Channels that need browser-impersonating TLS (cookie-based auth) use the spoof client.

10. **`classify_response`.** Inspect the HTTP status, headers, and body to decide what happened:
    - **Success** -- proceed.
    - **RateLimit** -- mark credential with cooldown, try next credential.
    - **AuthDead** -- attempt credential refresh (OAuth token rotation). If refresh succeeds, retry once. If that also fails, mark credential dead, try next.
    - **TransientError** -- mark credential with short cooldown, try next.
    - **PermanentError** -- return error to client.

11. **Retry / failover.** On retryable errors, loop back to step 7 with the next credential. Per-credential retry limit on 429 without `retry-after` is configurable (default: 3 attempts).

12. **`normalize_response`.** Channel-specific fixups on the response body before protocol transform. For example, DeepSeek maps `insufficient_system_resource` finish reason to `length`, and Vertex channels unwrap envelope wrappers.

13. **Protocol transform (response side).** If the original dispatch was `TransformTo`, convert the response body back from the upstream's protocol to the client's expected protocol.

14. **Usage recording.** Extract token counts from the response. Push a usage record to the async write sink (non-blocking). The usage sink worker batches writes to the database.

## Credential health

Each credential in a provider's pool has a health state:

- **healthy** -- available for selection. No active cooldowns.
- **cooldown** -- temporarily unavailable. Either a global cooldown or a per-model cooldown is active. The credential is skipped during selection but will recover automatically.
- **unavailable (dead)** -- the upstream returned 401/403 and credential refresh failed. The credential is excluded from selection entirely until manually re-enabled or a successful refresh occurs.

### Cooldown behavior

When a 429 arrives **with** a `retry-after` header, the cooldown duration matches the server's value exactly.

When a 429 arrives **without** `retry-after`, cooldown uses capped exponential backoff: 1s, 2s, 4s, 8s, ... up to 60s. The backoff counter resets to zero on the next success.

Cooldowns can be global (all models) or per-model, depending on what model the failing request targeted. If the request had a model, only that model is put on cooldown for the credential -- other models remain available. This is the "partial" state: the credential is healthy for most models but temporarily cooling down for a specific one.
