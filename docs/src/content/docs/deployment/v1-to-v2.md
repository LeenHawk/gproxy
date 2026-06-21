---
title: Migrating From v1 To v2
description: Migrate a v1 SQLite deployment to GPROXY v2 and understand what changes.
---

v2 includes a temporary v1 SQLite migrator so common single-node deployments can
replace the binary and start directly. On startup, v2 can detect an old
`data/gproxy.db`, read the v1 control-plane configuration, create a new v2
database, and move the old database aside as a backup.

This migrator is transitional and is planned for removal in 2.1. Move v1
instances through a v2 build that still includes the default `migrate-v1`
feature before upgrading to later v2 releases.

## When Automatic Migration Runs

Automatic migration runs only on the normal serve path and only when all of
these are true:

- the binary includes the default `migrate-v1` feature;
- no `import`, `export`, `update`, or `migrate-v1` subcommand is running;
- persistence is `db`;
- the target DSN is a real SQLite file;
- the file exists and looks like v1 schema: it has `global_settings` and
  `providers`, and does not have v2 `orgs`, `routes`, or `schema_migrations`.

The native v2 default matches the common v1 path: `GPROXY_PERSISTENCE=db`, and
without `GPROXY_DSN`, v2 uses `<data-dir>/gproxy.db`.

Automatic migration does not run for:

| Case | Result |
| --- | --- |
| Fresh install with no `gproxy.db` | Creates a normal v2 database. |
| Existing v2 database | Skips migration. |
| `--persistence=file` | No v1 SQLite migration. |
| PostgreSQL or MySQL target | Use explicit `migrate-v1 --to <dsn>`. |
| `sqlite::memory:` | No file exists to take over. |

## Drop-In Upgrade

Stop v1 before starting v2. If v1 used SQLite WAL, the migrator can account for
sidecar files, but the old process must stop writing first.

```bash
systemctl stop gproxy
cp data/gproxy.db data/gproxy.db.manualbak
install -m 0755 gproxy-v2 /usr/local/bin/gproxy

GPROXY_DATA_DIR=./data \
GPROXY_HOST=0.0.0.0 \
GPROXY_PORT=8787 \
gproxy
```

After success:

- `data/gproxy.db` is the new v2 database;
- the old v1 database is moved to `data/gproxy.db.v1.bak`;
- if that backup name exists, v2 uses the next available suffix such as
  `.v1.bak.2`.

The process is idempotent. Once the live database is v2 schema, later starts
skip the migration.

## Encrypted v1 Data

v1 used `DATABASE_SECRET_KEY` for encrypted `credentials.secret_json` or
`user_keys.api_key_ciphertext`. Provide the same value during migration:

```bash
DATABASE_SECRET_KEY='<v1 database secret>' \
GPROXY_MASTER_KEY='<base64-encoded 32-byte v2 master key, optional>' \
gproxy --data-dir ./data
```

The migrator opens v1 secrets with `DATABASE_SECRET_KEY`, maps the plaintext
control-plane data to a v2 import bundle, then writes through the v2 import path.
If `GPROXY_MASTER_KEY` is set, imported secrets are sealed with v2 rules. If it
is absent, v2 runs in plaintext secret mode and warns.

If encrypted v1 data is present and the correct `DATABASE_SECRET_KEY` is
missing, migration fails and the original database is not replaced.

## Offline Migration

Use the explicit subcommand for dry runs, PostgreSQL/MySQL targets, or controlled
maintenance windows:

```bash
gproxy migrate-v1 --from ./data/gproxy.db --dry-run

gproxy --data-dir ./data migrate-v1 --from ./data/gproxy.db

gproxy migrate-v1 \
  --from ./old/gproxy.db \
  --to 'postgres://gproxy:secret@db.internal:5432/gproxy'
```

Offline migration reads only the `--from` v1 SQLite file and writes mapped v2
records to the target DSN. It does not swap files and does not create
`.v1.bak`. Use an empty v2 target to avoid id collisions.

## What Migrates

v2 migrates control-plane configuration, not runtime history.

| v1 data | v2 result |
| --- | --- |
| `users` | `users`, attached to synthetic org `default`. |
| `user_keys` | `user_keys`, decrypted and rewritten through v2 import. |
| `providers` | `providers`, preserving id, name, channel, label, settings. |
| `credentials` | `credentials`, decrypted and re-sealed, default weight 100. |
| `models` | `provider_models`, preserving provider model metadata. |
| same `model_id` on multiple providers | one v2 route with multiple members. |
| `user_quotas` | user-scoped quotas. |
| `user_model_permissions` | route permissions with inherited v1 globs. |
| `user_rate_limits` | rate limits with inherited v1 route pattern. |
| `global_settings` | instance settings for proxy, logging, usage, update channel. |

Not migrated:

- usage billing history;
- upstream/downstream request logs;
- file records;
- credential health state;
- individual custom rules from v1 `routing_json`.

## Behavior Differences

| Difference | What to check after migration |
| --- | --- |
| v1 model becomes v2 route | Clients call the route name in aggregated mode. |
| same model across providers | v2 preserves multiple route members instead of treating one as an override. |
| default route strategy | synthetic routes use `failover`, weight 100, tier 0. |
| v1 `routing_json` | not translated; v2 seeds channel defaults and warns for unknown channels. |
| pricing | tiered/flex/scale/priority pricing collapses into v2 flat pricing fields. |
| TLS spoof | v1 `spoof_emulation` becomes a v2 instance-setting boolean. |

After migration, review provider channels, route members, routing rules, and
pricing in Console.

## Rollback

Stop v2, remove the v2 database files, and restore the v1 backup:

```bash
systemctl stop gproxy
rm -f data/gproxy.db data/gproxy.db-wal data/gproxy.db-shm
mv data/gproxy.db.v1.bak data/gproxy.db

[ -f data/gproxy.db.v1.bak-wal ] && mv data/gproxy.db.v1.bak-wal data/gproxy.db-wal
[ -f data/gproxy.db.v1.bak-shm ] && mv data/gproxy.db.v1.bak-shm data/gproxy.db-shm

systemctl start gproxy
```

Use the actual backup filename if the migrator created `.v1.bak.2` or another
suffix.
