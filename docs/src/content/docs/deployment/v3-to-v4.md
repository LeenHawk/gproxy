---
title: Migrating v3 to v4
description: Automatic migration when v4 starts with an existing v3 SQLite database.
---

**Replace the binary and start it with your existing configuration.** v4 recognizes
v3's SQLite database and upgrades it automatically. Keep the same data directory,
database path and `GPROXY_MASTER_KEY`; no export, new directory or password reset
is needed. Existing usernames, passwords and API keys continue to work.

## What startup does

Before binding its HTTP port, v4 takes a complete SQLite snapshot, including
committed WAL contents, and imports it into a temporary database beside the
original. It opens credentials with the existing master key and preserves the
users' Argon2 password hashes. Only after import and snapshot assembly succeed
does it replace the original database path. It keeps a `gproxy.db.v3-*.bak` backup
beside that path and logs its name.

A failed conversion does not replace the original. Correct the reported cause
(for example, restore the existing master key) and start again. An interruption
before replacement leaves v3 in place; after replacement, the v4 migration ledger
makes subsequent starts ordinary v4 starts. There is no repeated import.

Run one database writer during the upgrade, as for other schema migrations. Stop
v3 before starting v4. Running both deployments with the same OAuth credentials
can also invalidate refresh tokens, independently of database migration.

## What carries over

| Data | Result |
| --- | --- |
| Providers and credentials | Known channel names and configurations are translated; secrets are re-sealed using the configured key. |
| Routes and members | Public model names become route names. Additional names get separate routes with copied members. |
| Users, passwords, API keys, organizations and teams | Password hashes and API key digests are preserved. |
| Prices and budgets | Translated to v4's units. Values beyond 9 decimal places use the store's nearest-even rounding and are reported. |
| Permissions, rate limits and rewrites | Supported forms migrate; changes and unmappable forms are reported. |
| Usage history (SQLite import) | Imported in bounded batches with original token counts, attribution and settled costs; no repricing or quota settlement. Extra v3 fields remain in `metrics.v3`. |
| Captures, request logs, sessions and audit history | Retained in the v3 backup. Existing browser sessions need a fresh login. |

Providers with no translatable channel/configuration are left behind together with
their dependent rows, with an explicit report. This does not ignore database I/O
errors or incorrect encryption keys. Review the startup report for channel-specific
omissions or permission changes.

`gproxy migrate` uses the same automatic upgrade path. Once the database is v4,
SeaORM's `seaql_migrations` ledger records its schema changes. `migrate --status`
only reports status and never performs the v3 upgrade.

## Optional: import a separate backup or export

For a deliberate import into another instance, the explicit command is still
available:

```sh
gproxy --data-dir ./other-instance migrate
gproxy --data-dir ./other-instance import --from-v3 /path/to/v3-backup.db
```

A sealed source needs `--source-master-key` or `GPROXY_IMPORT_SOURCE_MASTER_KEY`
when using this explicit command. The destination must be empty or contain an
incomplete earlier import. `--skip-unmappable-providers` opts into the omission
policy automatic startup already uses.

JSON from v3's `POST /admin/api/export` is also accepted. Export with
`{"include_secrets": true}`. Unlike SQLite, those exports omit password hashes,
permissions, rate limits and provider models. Use `--admin-password` to set a
console password when importing a JSON export without hashes; direct SQLite
migration does not need it.
