---
title: Database Backends
description: Native and edge persistence backends, schema behavior, secret storage, and operational tradeoffs.
---

GPROXY v2 has one persistence trait with native SeaORM and edge libSQL/Turso
implementations.

Persistence stores control-plane data, authz data, logs, usage, rollups,
settings, tokenizers, transform rules, and provider credentials. The selected
cache backend is separate from persistence.

## Native backend

The native backend uses SeaORM and supports SQLite, PostgreSQL, and MySQL. Select
it with `GPROXY_PERSISTENCE=db`. If `GPROXY_DSN` is omitted, GPROXY derives
`sqlite://<absolute data_dir>/gproxy.db?mode=rwc`.

## Edge backend

The wasm edge bundle uses libSQL/Turso persistence when built with the edge
feature set. The schema is hand-written SQLite dialect DDL mirroring the native
SeaORM entities. Edge wrappers pass the platform-specific database URL/token to
the wasm entry point.

## DSN examples

```text
sqlite:///var/lib/gproxy/gproxy.db?mode=rwc
postgres://gproxy:secret@127.0.0.1:5432/gproxy
mysql://gproxy:secret@127.0.0.1:3306/gproxy
```

For local development, the default `db` mode creates a SQLite file under
`./data`:

```bash
./gproxy
```

## Schema creation and migrations

The native database backend creates tables on connect from the SeaORM entity
definitions and then runs the built-in migration tracker. The libSQL backend
uses matching `CREATE TABLE IF NOT EXISTS` SQL.

Important schema characteristics:

- provider names, route names, aliases, user names, and user-key digests are
  unique;
- `routing_rules` are unique per `(provider_id, operation, kind)`;
- quotas are unique per `(scope, scope_id)`;
- usage rollups use a composite unique index over granularity, bucket, and
  optional dimensions so concurrent first writes collide and retry into
  accumulation.

There is no separate operator command to run migrations in the current v2
binary. Startup creates/stamps/runs pending schema work and fails loudly if
that cannot complete.

## Major table groups

| Group | Tables |
| --- | --- |
| Providers | `providers`, `credentials`, `credential_statuses`, `credential_model_statuses`, `provider_models` |
| Routing | `routes`, `route_members`, `aliases` |
| Transform | `routing_rules`, `rule_sets`, `rules`, `provider_rule_sets` |
| Identity and authz | `orgs`, `teams`, `users`, `user_keys`, `route_permissions`, `rate_limits`, `quotas` |
| Usage and logs | `usages`, `usage_rollups`, `downstream_requests`, `upstream_requests`, `audit_logs` |
| Settings and tokenizers | `instance_settings`, `tokenizer_vocabs` |

JSON columns are stored as native JSON-like values in Rust records and as text
where the backend requires it. Decimal money fields are stored as decimal text.

## Secret storage

`GPROXY_MASTER_KEY` controls v2 sealed-secret mode. It must be standard base64
for exactly 32 decoded bytes.

- If set, provider credentials and user API-key ciphertext are sealed before
  storage.
- If absent, GPROXY runs in plaintext mode and logs a warning.
- User passwords are Argon2 hashes; the recovery override rehashes the supplied
  password before storing it.

Export decrypts secrets into the JSON bundle so `export | import` can
round-trip. Protect exported bundles.

`DATABASE_SECRET_KEY` is not the v2 runtime encryption key. It is used only by
the legacy v1 migration reader when a v1 database contains encrypted secrets.

## v1 migration

With the default feature set, the serve path can detect and migrate a legacy v1
SQLite database at the configured SQLite DSN before opening it as v2. The
explicit `migrate-v1` subcommand is also available when the feature is compiled.

If v1 used encrypted secrets, provide `DATABASE_SECRET_KEY` so the reader can
open them. Provide `GPROXY_MASTER_KEY` if the imported v2 rows should be sealed
under a v2 key.

## Retention and logs

Usage rows, request logs, audit logs, and rollups live in persistence. A
background retention task is spawned on the serve path and is a no-op until
instance settings define a retention window. Body capture can produce large
tables; enable it intentionally and set retention when operating a shared
service.
