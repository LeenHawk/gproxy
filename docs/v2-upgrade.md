# Upgrading v2 to v3 staging

The native v3 server recognizes a v2 SQLite database before opening the v3
store. The normal v2 launch command can keep `--persistence db`, `--data-dir`,
a file-backed `--dsn sqlite://PATH?mode=rwc`, and `--update-channel staging`.
Keep the existing `GPROXY_MASTER_KEY` if the source database is encrypted.
Do not combine this upgrade with a master-key rotation.

## Before upgrading

- Keep a copy of the currently installed v2 executable and its launch settings.
  Existing v2 self-updaters replace that executable without retaining it. The
  v3 startup migration cannot recover an executable already removed by v2.
- Stop other processes or instances using the database. The native entrypoint
  reserves its listening port before touching the database, and migration
  requires an exclusive SQLite lock.
- Allow disk space for a complete source backup and a separate migrated database.
- PostgreSQL, MySQL, remote libSQL, v1 databases and nonstandard SQLite DSNs are
  not automatic-upgrade inputs. Export/migrate them explicitly first.

## What startup does

1. Lock migration and checkpoint SQLite WAL before taking a consistent backup.
2. Create a private `gproxy-v2-backup-TIMESTAMP-PID` directory beside the database.
3. Copy the original database to `gproxy-v2.db` in that directory.
4. Import supported configuration, recoverable secrets and raw usage into a new
   v3 database under `candidate/`, using the existing migration implementation.
5. Verify import counts, SQLite integrity, references, runtime control state,
   usage row count and the exact total settled cost.
6. Flush and atomically replace the configured database only after validation.
   Preserve the backup and `report.txt`; restarting v3 does not reimport it.

Unknown populated tables and data the importer cannot represent stop automatic
migration. In particular, unmapped route permissions, rate limits, usage
rollups, credential quota history and custom tokenizer data are **not silently
discarded**. Request an explicit migration review for such databases.
Captured request/response logs and audit logs remain in the source backup;
they are not imported into the live v3 store. Browser and client login sessions
may need to be established again; ordinary recoverable API keys are preserved.

## Failure and rollback

Failures before replacement leave the v2 database in place. Inspect the reported
backup directory and `.gproxy-v2-upgrade-blocked` beside the database. The marker
prevents a process supervisor from repeatedly copying a failing database and
filling the disk. Correct the cause, then remove **only that marker** to retry.
Never delete the backup as a retry mechanism.

To roll back, stop v3, retain the current v3 database for investigation, reinstall
your saved v2 executable (or the same official v2 release), and restore the
backup `gproxy-v2.db` to the configured database path. Move any current `-wal`
and `-shm` files aside with the current database before restoring the backup.
Restore the old launch settings and master key, then start v2. Do not run v2
against a migrated v3 database. Usage written after the cutover stays in the
saved v3 database and is not merged back automatically.

## Publishing

Pushes to `main` publish the rolling `staging` release. Staging manifests use the
commit SHA as their update identity and commit-prefixed asset names; artifacts
are uploaded before the signed manifest is replaced. The previous manifest's
download targets are retained for in-flight updaters. Version-tag releases keep
their existing `releases`/`dev` behavior. Staging builds do not publish crates.
