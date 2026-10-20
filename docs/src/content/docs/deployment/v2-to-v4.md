---
title: Migrating v2 to v4
description: Migrate v2 to v4 with one script, using SQLite, PostgreSQL or MySQL at either end.
---

Use [migrate-v2-to-v4.py](https://github.com/LeenHawk/gproxy/blob/main/scripts/migrate-v2-to-v4.py). Both source and destination support SQLite, PostgreSQL and MySQL, including cross-database migrations.

One invocation runs **v2 source → SQLite snapshot → v3 SQLite → v4 SQLite → destination**. The source is read-only. Existing native importers upgrade accounts, credentials, API keys and usage before the complete v4 data is copied to the destination.

## Prepare

Stop the old instance and retain a database backup. The destination must be a **new SQLite file or an empty PostgreSQL/MySQL database**, never the original database. Keep the original `GPROXY_MASTER_KEY` for encrypted instances.

Prepare Python 3.10+, a [v3.0.22 executable](https://github.com/LeenHawk/gproxy/releases/tag/v3.0.22) and a [v4 executable](https://github.com/LeenHawk/gproxy/releases/latest). The v4 build must include the destination database driver. SQLite needs no additional Python packages; install the relevant driver for PostgreSQL or MySQL:

```sh
python3 -m pip install 'psycopg[binary]>=3,<4'  # PostgreSQL
python3 -m pip install 'PyMySQL[rsa]>=1,<2'     # MySQL
curl -fL https://raw.githubusercontent.com/LeenHawk/gproxy/main/scripts/migrate-v2-to-v4.py -o migrate-v2-to-v4.py
```

## Run the migration

SQLite example:

```sh
python3 migrate-v2-to-v4.py \
  --source ./old-data/gproxy.db \
  --target ./new-data/gproxy.db \
  --v3-bin ./v3/gproxy \
  --v4-bin ./v4/gproxy
```

Replace either database argument with `postgresql://user:password@host/database` or `mysql://user:password@host/database` for a remote database. To keep credentials out of command arguments, set `GPROXY_SOURCE_DSN` and `GPROXY_TARGET_DSN` and omit the corresponding flags. Supply the original encryption key through the environment when needed:

```sh
read -rsp 'Source database DSN: ' GPROXY_SOURCE_DSN; echo; export GPROXY_SOURCE_DSN
read -rsp 'Empty destination DSN: ' GPROXY_TARGET_DSN; echo; export GPROXY_TARGET_DSN
read -rsp 'Original master key (empty for plaintext): ' GPROXY_MASTER_KEY; echo; export GPROXY_MASTER_KEY
python3 migrate-v2-to-v4.py --v3-bin ./v3/gproxy --v4-bin ./v4/gproxy
```

The script creates and prints a private working directory; `--work-dir` can name a new directory explicitly. It looks for `gproxy-v3` and `gproxy` on PATH by default. Use `--source-schema` for a PostgreSQL source outside `public`; the PostgreSQL destination uses `public`. MySQL source tables must use InnoDB. MySQL DSNs accept `ssl-mode`, `ssl-ca`, `ssl-cert` and `ssl-key` for TLS.

## After completion

Intermediate databases and stage reports remain in the working directory. Verification checks account identities, API-key digests and historical costs; the final PostgreSQL/MySQL copy also compares every data table's row count and row-content checksums. Review `v3-import.log`, `v4-import.log` and `*-verification.json`, then start v4 against the destination with the same master key.

Existing conversion boundaries still apply: some old permissions and rules need reconfiguration; retained usage details migrate, but deleted details cannot be rebuilt from aggregates. Old request logs, sessions, audit records and caches do not become corresponding v4 records. v4 stores costs to 9 decimal places and retains exact historical values in `metrics.v3.cost`. See [v2 → v3](/deployment/v2-to-v3/) and [v3 → v4](/deployment/v3-to-v4/).

Failures stop the script and preserve its files without deleting the source or automatically clearing the destination. Diagnose the failure, then retry with a new working directory and empty destination. Keep the working directory private. v3.0.22 briefly receives the source key through process arguments; do not use shell tracing.
