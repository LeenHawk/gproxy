---
title: Migrating v3 to v4
description: Automatic migration when v4 starts with an existing v3 database.
---

**Replace the binary and start it with your existing configuration.** v4 recognizes
v3 databases on SQLite, PostgreSQL, MySQL, and D1 and upgrades them automatically. Keep the same data directory,
database path and `GPROXY_MASTER_KEY`; no export, new directory or password reset
is needed. Existing usernames, passwords and API keys continue to work.

## SQLite file migration

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

## PostgreSQL, MySQL, and D1

Keep the original connection URL or D1 binding and master key. No export or new database is required. Native PostgreSQL / MySQL builds must include the corresponding driver.

Migration validates source configuration and credential encryption, then renames the old tables to `gproxy_v3_<original-name>`. These tables retain the source data. New v4 tables use the original names. Configuration, identity, and historical usage are converted before the instance opens to traffic.

- **PostgreSQL** uses a session advisory lock and renames the source tables in one transaction.
- **MySQL** keeps its session lock across implicit DDL commits and renames all source tables with one `RENAME TABLE` statement.
- **D1** uses transactional batches for renaming, plus a migration lease and a holder check on each write batch. An interrupted lease expires within 60 seconds.

`gproxy_v3_upgrade` stores progress. Its `report_json` field retains skipped records and semantic-change warnings. Identity and usage imports checkpoint between batches. After interruption, the next startup or Worker request resumes unfinished work. A partially migrated instance does not serve traffic or bootstrap a new administrator.

After a failure, old tables remain under archive names and new tables may contain completed portions of the import. Fix the reported error and restart to resume. Do not delete the progress table or rename archives during migration. To roll back to v3, stop all instances and restore the pre-upgrade backup; do not point v3 at a partially migrated database. Keep archived tables until the upgrade has been verified.

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
