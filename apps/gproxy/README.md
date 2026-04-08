# App Layer Overview

## Architecture Overview

The gproxy application layer can be understood through the following pipeline:

| Layer | Crate / Directory | Primary Responsibility |
| --- | --- | --- |
| Domain layer | `crates/gproxy-core` | Provides in-memory domain services for identity, policy, quota, routing, files, configuration, and an optional Redis backend. |
| Persistence layer | `crates/gproxy-storage` | Manages database connections, schema sync, query repositories, and write events through SeaORM. |
| API layer | `crates/gproxy-api` | Organizes login, Admin, User, and Provider HTTP/WebSocket routes with Axum and handles bootstrap. |
| Application entry | `apps/gproxy` | Parses CLI arguments and environment variables, connects to the database, creates `AppState`, starts background workers, and serves both the API router and the embedded `/console` frontend. |

`AppState` and `AppStateBuilder` in `crates/gproxy-server` are what actually wire these layers together at runtime. In `apps/gproxy/src/main.rs`, the process first creates `GlobalConfig`, `SeaOrmStorage`, the SDK engine, and the workers, then combines the `gproxy-core` services into shared state and finally exposes the API through `gproxy_api::api_router`.

## The Six Domain Services

The following table lists the six core in-memory services:

| Service | Source | Responsibility |
| --- | --- | --- |
| Identity | `crates/gproxy-core/src/identity.rs` | Maintains in-memory snapshots of users and API keys; API keys are first hashed with SHA-256 plus a domain separator before becoming HashMap keys, avoiding plaintext lookups; handles authentication, per-user key lookup, atomic replacement, and single-item CRUD for users and keys. |
| Policy | `crates/gproxy-core/src/policy.rs` | Maintains user model permissions, file permissions, and rate-limit rules; checks model access, provider access, file-upload permissions, and rate-limit rules by model pattern. |
| Quota | `crates/gproxy-core/src/quota.rs` | Uses `DashMap` to maintain `(quota_total, cost_used)` for each user; supports quota checks, cost accumulation, full replacement, and snapshot export. |
| Routing | `crates/gproxy-core/src/routing.rs` | Maintains model catalogs, model aliases, `provider_name -> provider_id/channel` mappings, and indexes from providers to credential ID lists; handles alias resolution, model lookup, and credential lookup. |
| File | `crates/gproxy-core/src/file.rs` | Maintains user file records and cached Claude file metadata; supports looking up active files by user, provider, and `file_id`, as well as batch replacement and single-item updates. |
| Config | `crates/gproxy-core/src/config.rs` | Holds the current global configuration with `ArcSwap<GlobalConfig>` and provides atomic reads and swaps. |

These services are the holders of the in-memory source of truth. Persisted data in the database is provided by `gproxy-storage` and reloaded into these services during startup or on `/admin/reload`; authentication, permission checks, quota enforcement, and route resolution on the request path read directly from these in-memory services.

## Background Workers

| Worker | Source | Trigger | Responsibility |
| --- | --- | --- | --- |
| UsageSink | `apps/gproxy/src/workers/usage_sink.rs` | mpsc queue, flushing at 100 items or every 500 ms. | Writes usage records asynchronously in batches so data-plane requests do not block on database writes; on shutdown it closes the receiver, drains remaining messages, and performs one final flush. |
| QuotaReconciler | `apps/gproxy/src/workers/quota_reconciler.rs` | Polls every 30 seconds. | Reads the source-of-truth quota state from the database and repairs the local `QuotaService`, mainly for admin-side quota changes or cross-instance cost growth. |
| HealthBroadcaster | `apps/gproxy/src/workers/health_broadcaster.rs` | Subscribes to SDK `EngineEvent` with a 500 ms debounce window. | Watches credential health-state changes, resolves `(provider, credential index)` into database credential IDs, and persists them into `credential_statuses`. |
| RateLimitGC | `apps/gproxy/src/workers/rate_limit_gc.rs` | Polls every 60 seconds. | Cleans expired window counters from the in-memory rate-limit state. |

All workers share a `watch<bool>` shutdown signal through `WorkerSet`. On application exit the shutdown signal is sent, and the process waits up to five seconds for workers to drain their buffers; a warning is logged if the timeout is exceeded.

## Startup Flow

The startup order in the source can be summarized as:

`CLI arguments -> DB connection -> Bootstrap -> Workers -> HTTP Server`

The detailed sequence in `apps/gproxy/src/main.rs`:

1. Initialize tracing with a default log level of `info`; `RUST_LOG` can override it.
2. Parse CLI arguments and environment variables to obtain `host`, `port`, `dsn`, `config`, `data_dir`, `proxy`, `spoof`, bootstrap admin username/password/API key, and `DATABASE_SECRET_KEY`.
3. Resolve the database DSN. The default DSN is derived from `data_dir/gproxy.db` in the format `sqlite://<data_dir>/gproxy.db?mode=rwc`.
4. Create the data directory, connect to the database, and run `storage.sync()` for schema synchronization.
5. If neither `dsn` nor `data_dir` was explicitly passed via the CLI and `global_settings` in the database persists a different DSN, reconnect to that database during startup.
6. Build `GlobalConfig` and optionally connect to Redis. Redis is initialized only when the `redis` feature is compiled in and `GPROXY_REDIS_URL` is set.
7. Create the `UsageSink` channel first and inject the sender into `AppStateBuilder`; the actual worker starts after `AppState` construction so it always reads the latest `storage`.
8. Build the SDK engine and `AppStateBuilder`, injecting `storage`, `config`, and `usage_tx`.
9. Execute bootstrap. If `global_settings` already exists in the database, call `reload_from_db` to restore the full in-memory state; otherwise, if the TOML file pointed to by `GPROXY_CONFIG` exists, initialize from TOML; failing that, create a minimal runtime configuration, a real admin user, and a bootstrap admin API key.
10. Write back the explicitly provided host, port, proxy, spoof, dsn, and data_dir into the global configuration. Admin identity is no longer stored in `global_settings` but in `users.is_admin` and the corresponding user key; on first startup, if the bootstrap password or API key is missing, one is generated and printed once.
11. Start the remaining workers: `QuotaReconciler`, `RateLimitGC`, `HealthBroadcaster`.
12. Build `gproxy_api::api_router(state)`, merge the embedded `/console` router from `apps/gproxy/src/web.rs`, bind to `host:port`, and start the Axum HTTP server.
13. On `Ctrl+C` or `SIGTERM`, begin graceful shutdown: stop the HTTP server first, then notify workers to shut down and wait for them to drain.

## Embedded Console Workflow

The embedded console assets live in `apps/gproxy/web/console/` and are served by `apps/gproxy/src/web.rs`.

When the frontend changes:

```bash
cd frontend/console
pnpm install
pnpm build
```

This builds the SPA and syncs the assets into the embed directory. After that, run `cargo run -p gproxy` or `cargo build -p gproxy`.

At runtime:

- open `/console`
- log in through `/login`
- use the returned session token through browser-managed requests to `/admin/*` and `/user/*`

## Runtime Collaboration

- `gproxy-api` authentication and routing middleware read `IdentityService`, `PolicyService`, `RoutingService`, and `ConfigService` directly from `AppState`.
- `gproxy-storage` is the persisted source of truth, and `reload_from_db` / `seed_from_toml` translate database or TOML state into the in-memory models owned by `gproxy-core`.
- `QuotaService`, the rate-limit counters, and the file cache are hot-path request data, and quota is additionally repaired periodically by `QuotaReconciler`.
- Actual upstream forwarding comes from the SDK engine, but provider names, aliases, permissions, and credential indexes are owned by the application-layer state.
