---
title: "Storage & Cache Backends"
description: "The four SQL backends and how to select them, schema migrations and table groups, cache backends and multi-instance needs, backups, retention, and edge limits"
---

`gproxy-store` holds one schema catalog and one query layer (SeaQuery)
for every backend. Dialect differences — integer widths, indexed
`VARCHAR(255)` on MySQL, `PRAGMA foreign_keys` on SQLite — are applied
when statements are rendered, and backend parity is covered by the
store's test scenarios. The cache is a separate service, selected
independently of persistence.

## Selecting a Backend

| `GPROXY_PERSISTENCE` | Connection | Notes |
| --- | --- | --- |
| `sqlite` (default) | `<data-dir>/gproxy.db` | Bundled SQLite, one file. Foreign keys enabled. |
| `libsql` | `GPROXY_LIBSQL_URL` + `GPROXY_LIBSQL_AUTH_TOKEN` | Hrana over HTTP at `<url>/v2/pipeline`; works with Turso and any libSQL server. The only backend on edge. |
| `postgres` | `GPROXY_DSN=postgres://user:<password>@host:5432/gproxy` | `tokio-postgres`, a single connection behind a lock; each migration batch runs in a transaction. The connection is opened without TLS; keep the database on a private network or a local socket. |
| `mysql` | `GPROXY_DSN=mysql://user:<password>@host:3306/gproxy` | `mysql_async` connection pool with rustls TLS support; migration batches run in a transaction. |

```bash
GPROXY_PERSISTENCE=postgres \
GPROXY_DSN='postgres://gproxy:<password>@db.internal:5432/gproxy' \
gproxy
```

Column types: `Integer` is `INTEGER` on SQLite and `BIGINT` elsewhere;
`Text` is `TEXT`, or `VARCHAR(255)` on MySQL when the column is indexed;
`Blob` is binary, or `VARBINARY(255)` on MySQL when indexed. Timestamps
are Unix seconds in integer columns, money is decimal text, and JSON is
text.

## Migrations

Startup opens the backend and migrates before anything else; there is no
separate migration command. `schema_migrations(version, applied_at)`
records each applied version. The history must be contiguous, and a
database newer than the binary is refused with
`database schema is newer than this binary`.

| Version | Name | Adds |
| --- | --- | --- |
| 1 | `Initial` | The complete current v3 schema listed below. The development-time versions that preceded the first v3 release were flattened; migrations added after release start at version 2. |

Version 1 is the current schema. The self-update manifest carries a
minimum data version that is compared against this number before an
update is applied.

This ladder upgrades v3 stores only. `gproxy migrate --from-v2` is a separate
data importer: it reads the v2 SQLite source without modifying it, opens a
current v3 target (thereby creating Initial v1), then maps and writes the v2
entities. It does not replay superseded v3 development migrations.
Pre-release v3 stores created with the removed 15-version ladder must be
recreated; they were never a supported migration source.

## Tables by Group

| Group | Tables | Notes |
| --- | --- | --- |
| Providers and routing | `providers`, `credentials`, `provider_models`, `routes`, `route_members`, `exposed_models`, `aliases` | Provider, route and exposed-model names are unique. `credentials` stores the sealed envelope (`ciphertext`, `wrapped_key`, `payload_nonce`, `key_nonce`) and a `version` for compare-and-swap on rotation. |
| Rules | `routing_rules`, `rule_sets`, `rules`, `provider_rule_sets` | `routing_rules` is unique per `(provider_id, operation, kind)`; `origin` separates channel-seeded rows from operator rows. |
| Pricing | `price_rules`, `price_rates` | See [Pricing & Tiers](/reference/pricing/). |
| Identity | `organizations`, `teams`, `users`, `user_keys`, `user_sessions`, `permissions`, `rate_limits`, `quotas` | Team names are unique per organization. `user_keys` holds the unique digest, a display `prefix` and the sealed key. One quota row per subject. |
| Quota runtime | `quota_windows`, `quota_settlements`, `credential_quota_cycles`, `credential_quota_cycle_models` | Windows are unique per `(quota, kind, start)`; settlements per `(request, window)`. Cycles track upstream quota readings per credential. |
| Usage | `usage_rows`, `usage_rollups` | One row per request id with token columns, `metrics_json`, `dimensions_json`, decimal `cost`, `usage_source`, `ended`, `latency_ms`. Rollups are unique per `(granularity, bucket_start, dimension_key)`. |
| Logs | `request_logs`, `wire_logs` | One downstream exchange per request id; one wire log per upstream attempt. Bodies are blobs, present only when body capture is on. |
| Admin | `admin_audit_events`, `credential_health`, `surface_bindings`, `settings` | Health is per `(credential, model)`. Bindings pin service-surface resources to the credential that created them. `settings` is a key → JSON map. |
| Tokenizers | `tokenizer_vocabs`, `tokenizer_auth` | Cached vocabularies and the sealed Hugging Face token. |
| OAuth | `oauth_grants`, `oauth_codes`, `oauth_tokens`, `oauth_devices` | Issuer state for the emulated vendor-auth surfaces. |

## Cache Backends

| Backend | Selected by | Scope |
| --- | --- | --- |
| In-process | default on native | One process. |
| Redis | `GPROXY_REDIS_URL` | Shared. `redis` crate with a connection manager; rustls TLS. |
| Upstash REST | `UPSTASH_URL` + `UPSTASH_TOKEN` | Shared. One HTTPS request per command; native and edge. |
| libSQL table | automatic when persistence is `libsql` and neither of the above is set | Shared through the database: `gproxy_kv(k, v, expires_ms)`. |

The cache contract is `get`, `set`, `delete`, `incr`,
`compare_incr_and_set` and `compare_and_swap`, all with optional TTL. It
carries admission state (`gproxy:admission:{request_id}`), pending quota
estimates (`gproxy:quota-pending:{window}`), request-rate windows
(`gproxy:rate:{limit}:{window_start}`), credential RPM/TPM windows
(`gproxy:credential-rate:{credential}:{rpm|tpm}:{minute}`), settlement
de-duplication for polled video jobs, credential refresh leases, and
session-affinity pins. Two instances with separate in-process caches would
each enforce limits alone and each refresh the same OAuth token, so a
multi-instance deployment needs Redis, Upstash or the libSQL table.

Control-plane snapshots are rebuilt by the instance that made a change. The
instance then increments `gproxy:invalidate` in the shared cache; native
instances poll it once per second and edge isolates check it on each request,
reloading their snapshot when the version changes.

## Backups

- SQLite: stop the process and copy `<data-dir>/gproxy.db`, or use
  SQLite's online backup (`sqlite3 gproxy.db ".backup gproxy-backup.db"`).
  Keep the master key with the copy; a sealed database is unreadable
  without `GPROXY_MASTER_KEY`.
- PostgreSQL and MySQL: the database's own dump tools.
- libSQL/Turso: the platform's snapshots.
- Logical export: console → Settings → Configuration import and export
  (`POST /admin/api/export`, `POST /admin/api/import`). The export carries
  providers, credentials, keys, quotas, pricing, routes, aliases and rule
  sets. With `include_secrets` it also carries credential and key
  secrets, sealed under the exporting instance's key; import opens them
  with the source master key and re-seals them under the local key.
  Usage, logs and audit rows are not exported; embedded default price
  rows are omitted.

## Retention and Size Pressure

A sweep runs every 5 minutes on native hosts. It deletes `usage_rows`,
`request_logs` and `wire_logs` older than `retention_days` (5,000 rows
per table per sweep), measures the database (`page_count × page_size` on
SQLite and libSQL, `pg_database_size` on PostgreSQL, `information_schema`
sizes on MySQL), and when the size exceeds `max_database_size_mb` deletes
the 5,000 oldest `request_logs` and `wire_logs` rows — never usage. Unset
settings behave as 36,500 days and 1,024 MiB. The sweep needs a
background spawner, so it does not run on edge; bound edge storage with
your database provider's tools.

## What the Edge Host Supports

The wasm host compiles only the libSQL backend and the libSQL and Upstash
caches. There is no SQLite file, no PostgreSQL or MySQL driver, no
in-process cache, no Redis client, no cleanup sweep, and no Hugging Face
vocabulary registry.
