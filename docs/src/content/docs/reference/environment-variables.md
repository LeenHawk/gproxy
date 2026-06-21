---
title: Environment Variables
description: Runtime, bootstrap, storage, update, and development environment variables supported by gproxy v2.
---

gproxy v2 is configured at process startup by CLI flags and environment
variables. The native binary uses `clap`; for flags that declare an environment
variable, the explicit CLI flag wins over the environment value.

Most live configuration is not environment-driven after startup. Providers,
credentials, models, routes, aliases, authz rules, quotas, pricing, transform
rules, and instance settings are stored in persistence and edited through the
console, admin API, or JSON import/export.

## Server

| Variable | Default | Description |
| --- | --- | --- |
| `GPROXY_HOST` | `127.0.0.1` | Bind host. IPv6 addresses need bracket notation when passed as a CLI flag, for example `[::1]`. |
| `GPROXY_PORT` | `8787` | Bind port. |
| `GPROXY_MAX_IN_FLIGHT` | `1024` | Maximum concurrent gateway requests. Excess gateway requests are load-shed with `503`; admin and ops endpoints remain outside this gateway limiter. |
| `GPROXY_MAX_ATTEMPTS` | `6` | Per-request failover attempt cap. A forced credential refresh for an auth-dead candidate does not count as a new logical candidate. |
| `GPROXY_INSTANCE_ID` | `0` | Numeric instance identifier used where rows need per-instance partitioning. Use distinct values in a multi-node fleet. |
| `GPROXY_TRUSTED_PROXIES` | Empty | Comma-separated IP addresses whose `x-forwarded-for` / `x-real-ip` headers are trusted, in addition to loopback. |
| `GPROXY_CORS_ORIGINS` | Empty | Comma-separated exact origins allowed for cross-origin admin console/API use. Empty means same-origin only. |

## Persistence and cache

| Variable | Default | Description |
| --- | --- | --- |
| `GPROXY_PERSISTENCE` | `db` | Native persistence backend: `db` or `file`. `db` uses SeaORM and defaults to a SQLite file if no DSN is supplied. `file` stores one JSON file per table and is single-instance only. |
| `GPROXY_DATA_DIR` | `./data` | Data directory. Used by the file backend, the default SQLite DSN, v1 migration backup/temp files, and self-update staging. |
| `GPROXY_DSN` | Generated | Database DSN for `GPROXY_PERSISTENCE=db`. If omitted, gproxy uses `sqlite://<absolute data_dir>/gproxy.db?mode=rwc`. |
| `GPROXY_REDIS_URL` | Empty | Redis URL for the shared cache backend when the binary is built with the `cache-redis` feature. If omitted, the native default is in-process memory cache. |
| `GPROXY_MASTER_KEY` | Empty | Standard base64-encoded 32-byte key used to open and seal stored secrets. If absent, gproxy runs in plaintext-secret mode and logs a warning. This variable is env-only; there is no CLI flag. |

## Upstream and routing support

| Variable | Default | Description |
| --- | --- | --- |
| `GPROXY_UPSTREAM_PROXY_URL` | Empty | Native outbound proxy URL for upstream provider requests. Provider or credential proxy settings can override it. Edge deployments ignore this native HTTP-client setting. |
| `GPROXY_IMPORT_FILE` | Empty | Serve-path first-boot import hook. If set and the store has no providers and no users, gproxy imports this JSON bundle before admin bootstrap. It is skipped once the store is populated. |

## Admin bootstrap

| Variable | Default | Description |
| --- | --- | --- |
| `GPROXY_ADMIN_USER` | `admin` | Admin username used by first-boot bootstrap and by the recovery override. |
| `GPROXY_ADMIN_PASSWORD` | Empty | If set, force-upserts/resets the named admin user on every startup. The password must satisfy the same policy as the admin API. Remove it after recovery. If unset and the users table is empty, gproxy creates an admin with a random password and prints it once. |

There is no `GPROXY_ADMIN_API_KEY` bootstrap variable in the current v2 native
path. User API keys are generated or managed through the admin/portal APIs, or
imported through a JSON bundle.

## Self-update

| Variable | Default | Description |
| --- | --- | --- |
| `GPROXY_UPDATE_REPO` | Empty | GitHub `owner/repo` used by admin-triggered self-update and by the `gproxy update` subcommand. If unset on the serve path, admin update check/apply is disabled. |
| `GPROXY_UPDATE_CHANNEL_SERVE` | `releases` | Serve-path self-update channel: `releases` or `staging`. |
| `GPROXY_UPDATE_CHANNEL` | `releases` | Channel for the `gproxy update` subcommand. It intentionally differs from the serve-path env var to avoid a `clap` collision. |
| `GPROXY_UPDATE_RESTART` | `supervisor` | Restart mode for `gproxy update apply`: `supervisor`, `re-exec`, or `none`. |

`GPROXY_UPDATE_PUBKEY` is a build-time variable used when compiling a binary
with an embedded update verification public key. It is not read as a runtime
configuration variable.

## Development and migration

| Variable | Default | Description |
| --- | --- | --- |
| `GPROXY_INSECURE_COOKIES` | Empty | Development escape hatch for local plaintext HTTP. When set to `1`, admin session cookies can be issued without the `Secure` flag. Do not use it for production HTTPS deployments. |
| `DATABASE_SECRET_KEY` | Empty | v1 migration-only key name. If a legacy v1 database stored encrypted secrets, the v1 migration reader uses this key to decrypt them before re-sealing under `GPROXY_MASTER_KEY`. |
| `RUST_LOG` | `info` | Standard `tracing_subscriber` filter used by native logging. |

## Edge wrappers

The wasm edge entry points are configured by the platform wrapper rather than
by `clap`. Current deployment templates pass a Turso/libSQL database URL and
token to the wasm persistence backend, optionally pass an Upstash cache URL and
token, and can pass `GPROXY_MASTER_KEY` for sealed secrets. Check the edge
deployment page for the exact platform variable names because they are wrapper
specific.

## Example

```bash
GPROXY_HOST=0.0.0.0 \
GPROXY_PORT=8787 \
GPROXY_PERSISTENCE=db \
GPROXY_DATA_DIR=/var/lib/gproxy \
GPROXY_DSN='postgres://gproxy:secret@db.internal:5432/gproxy' \
GPROXY_MASTER_KEY="$GPROXY_MASTER_KEY" \
GPROXY_ADMIN_PASSWORD="$RECOVERY_PASSWORD" \
./gproxy
```

For first-boot seeding, prefer a JSON bundle:

```bash
GPROXY_IMPORT_FILE=/etc/gproxy/import.json ./gproxy
```
