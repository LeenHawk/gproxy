---
title: Database Backends
description: Supported databases, DSN formats, and at-rest encryption in gproxy.
---

`gproxy-storage` compiles in three database backends via **SeaORM** and
**SQLx**. You pick one by setting `GPROXY_DSN` (or the TOML `dsn` field).

## Supported backends

| Database | DSN prefix | Notes |
| --- | --- | --- |
| SQLite | `sqlite:` | Default mode. If `GPROXY_DSN` is not set, GPROXY generates a SQLite file DSN automatically under `GPROXY_DATA_DIR`. |
| PostgreSQL | `postgres:` | Provided by `sqlx-postgres` and the SeaORM Postgres feature. |
| MySQL | `mysql:` | Provided by `sqlx-mysql` and the SeaORM MySQL feature. |

## DSN examples

```text
sqlite://./data/gproxy.db?mode=rwc
postgres://gproxy:secret@127.0.0.1:5432/gproxy
mysql://gproxy:secret@127.0.0.1:3306/gproxy
```

For SQLite, the `?mode=rwc` flag tells SQLx to open the file
read/write and create it if missing — convenient for first-run
bootstrap.

## Connection lifecycle

When GPROXY starts, `SeaOrmStorage::connect()`:

1. Optionally loads the database encryptor corresponding to
   `DATABASE_SECRET_KEY`.
2. Applies per-database connection tuning parameters (pool size,
   timeouts, pragmas for SQLite).
3. Connects to the database and runs `sync()` to reconcile the schema
   with the compiled-in entity definitions.

There are no separate "run migrations" commands — the schema sync is
part of startup. If the sync fails, GPROXY aborts with a loud error
rather than running with a partial schema.

## At-rest encryption

Set `DATABASE_SECRET_KEY` to turn on the database encryptor. When it is
enabled, GPROXY encrypts the following fields with
**XChaCha20-Poly1305** before writing them:

- Provider credentials
- User passwords (in addition to the Argon2 hash, the hash itself is encrypted)
- User API keys

Losing the key means losing the ability to decrypt existing rows. Back
it up out of band (secrets manager, sealed file, …).

:::caution
Rotating `DATABASE_SECRET_KEY` is **not** automatic. If you change it,
existing encrypted rows will fail to decrypt at read time. Re-create or
re-import the affected credentials after rotating.
:::

## Choosing a backend

- **SQLite** is the right choice for single-node deployments, dev
  environments, and small shared instances. It needs zero
  infrastructure and is fast enough for most LLM proxy workloads
  because the hot path rarely writes.
- **PostgreSQL** is the recommended choice for HA setups, multi-writer
  deployments, or anywhere you already have a managed Postgres.
- **MySQL** is supported and functional; use it when it's what your
  ops team runs.
