---
title: Migrating v2 PostgreSQL to v4
description: An offline migration tool for moving a v2 PostgreSQL database to a new v4 PostgreSQL database.
---

The standalone script is currently distributed from the development branch; the commands below use that version.

Download the standalone [migration script](https://github.com/LeenHawk/gproxy/blob/dev/scripts/migrate-v2-postgres.sh). It embeds its Python data bridge, so only one script is needed. This is an offline bridge for the v2.3.2 schema:

```text
v2 PostgreSQL -> temporary v2 SQLite -> v3.0.22 SQLite -> new v4 PostgreSQL
```

The PostgreSQL source is read in one read-only, repeatable-read transaction.
The script does not alter it. Credentials remain sealed in the snapshot; the
existing v3/v4 importers decrypt and re-seal them with your master key.
Neither step starts an HTTP server. No binaries are downloaded or installed
by the script.
The SQLite file is an import interchange, not a replacement for `pg_dump`:
PostgreSQL indexes, defaults and constraints are not reproduced for restoration.

## Download and requirements

Save the script locally before running it:

```sh
curl -fL https://raw.githubusercontent.com/LeenHawk/gproxy/dev/scripts/migrate-v2-postgres.sh -o migrate-v2-postgres.sh
```

- Bash, Python 3.10+ with SQLite, and psycopg 3:

  ```sh
  python3 -m venv .migration-venv
  .migration-venv/bin/pip install 'psycopg[binary]>=3,<4'
  export PYTHON="$PWD/.migration-venv/bin/python"
  ```

- A **v3.0.22** executable and a current stable v4 executable for your platform.
  Download/extract them into separate directories from
  [v3.0.22](https://github.com/LeenHawk/gproxy/releases/tag/v3.0.22) and
  [stable releases](https://github.com/LeenHawk/gproxy/releases/latest).
  The v4 executable must include PostgreSQL support. Keep the v3 binary pinned:
  this bridge targets its source schema and command syntax.
  The official v4.0.2 Linux CLI lacks the PostgreSQL driver; use a current
  stable CLI (the bridge was validated with v4.1.8).
- A complete `pg_dump` backup made before the attempted upgrade, the original
  master key, and enough local disk space for both SQLite databases (including
  request logs). v3's importer loads usage details into memory, so allow memory
  proportional to retained usage history.
- A new, dedicated, empty PostgreSQL target database using the `public` schema.
  Do not start GPROXY against it before import. The importer creates its tables.

Stop all GPROXY instances writing to the source before the final preparation.
Prefer restoring the original v2 dump into a temporary PostgreSQL database and
using a SELECT-only account to read it. If you already launched v4 against the
old database, use that original backup, not the mixed database. Do not send
your dump, credentials, or master key to anyone to run this tool.

## 1. Prepare and inspect

Set the connection URL and original base64 `GPROXY_MASTER_KEY` in your shell.
The following prompts avoid writing them into shell history:

```sh
read -rsp 'Source PostgreSQL URL: ' GPROXY_V2_SOURCE_DSN; echo
export GPROXY_V2_SOURCE_DSN
read -rsp 'Original master key (empty if plaintext): ' GPROXY_MASTER_KEY; echo
export GPROXY_MASTER_KEY

# Optional if the v2 tables are outside public:
# export GPROXY_V2_SCHEMA=your_schema

bash migrate-v2-postgres.sh prepare ./migration-work /absolute/path/to/v3/gproxy
```

`migration-work` must not exist. It is created with private permissions, and all
output stays there. The command copies all source tables in bounded batches,
checks row counts, adds the older schema's missing fields **only in SQLite**,
then runs the v3 preflight, import, integrity and usage checks. Legacy
`image_price` is preserved as a price per `image_outputs` item, alongside the
token prices per million; it is not renamed to a token price.

Review these files before proceeding:

- `snapshot-report.json`: source counts and temporary schema adjustments.
- `v3-preflight.log`, `v3-import.log`: conversion results, skipped tables and
  permission/route changes.
- `v3-verification.json`: core configuration counts and exact historical cost.

Keep the entire directory private. It contains database copies and reports.
v3.0.22 accepts its source key only through `--from-v2-master-key`, so that key
is briefly present in the v3 process arguments. Run preparation on a trusted
machine; do not use shell tracing (`bash -x`). The script does not print keys.

## 2. Import into a new PostgreSQL database

Keep the same master key and prepared directory:

```sh
read -rsp 'New empty target PostgreSQL URL: ' GPROXY_V4_TARGET_DSN; echo
export GPROXY_V4_TARGET_DSN

bash migrate-v2-postgres.sh import ./migration-work /absolute/path/to/v4/gproxy
```

The script checks the key fingerprint and refuses a nonempty target before
invoking v4. It imports configuration, identity and usage through
`import --from-v3`, then verifies the imported core entity IDs, password hashes,
API key digests, usage count, original request IDs and costs (including v4's
rounded stored amounts). Review `v4-import.log` and
`v4-verification.json`. Unsupported providers fail the explicit import; the
script does not opt into silently omitting them.

Only after reviewing the reports, point your v4 deployment at the new database
with the same master key. Check your existing login, API key access, provider
credentials, routes, permissions and historical usage before resuming traffic.
Do not run both deployments using the same OAuth refresh tokens.

## What the bridge preserves and what it does not

The bridge inherits the existing migration rules; it does not promise identical
behavior for every v2 feature. Providers, credentials, routes, users, recoverable
API keys, password hashes and retained usage details pass through those rules.
See [v2 to v3](https://github.com/LeenHawk/gproxy/blob/main/docs/v2-upgrade.md) and
[v3 to v4](/deployment/v3-to-v4/) for the conversion boundaries.

- v4 rounds stored monetary values to 9 decimal places. Exact original usage
  costs remain in `metrics.v3.cost`; verification compares those originals.
- Deleted usage details cannot be reconstructed from old aggregates. v2 usage
  rollups and historical quota cycles are retained in the snapshot, not imported.
- Request logs reach the temporary v3 database but do not become v4 captures.
  Sessions, audit logs, credential health and caches are not carried over.
- Some permissions, rate limits, rewrites and provider settings change or cannot
  be translated. Read both migration reports and recreate required policies
  before serving traffic. Passing count checks is not a policy equivalence proof.

## Failure, retry and rollback

A failed command exits nonzero and retains all work files. It never deletes a
database or tries to undo a partial import. No target connection is needed for
`prepare`, and the source is never written in either step.

If preparation fails, inspect the reports, fix the reported cause, and prepare
into a **new work directory**. If import fails, retain the partially populated
target and its logs for diagnosis. After fixing the cause, use a **new empty
target and new work directory** for another complete run. Reusing a populated
target is deliberately refused; do not use `--merge` to bypass this check.

Before cutover, rollback means keeping the original deployment and database.
After cutover, stop v4 and restore the original v2 deployment/backup; usage
written to v4 is not merged back automatically. Keep the original dump and work
directory until the migration has been checked and your backup policy allows
their removal.
