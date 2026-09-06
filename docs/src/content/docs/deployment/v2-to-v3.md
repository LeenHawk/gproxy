---
title: "v2 to v3 Migration"
description: "Move a v2 deployment to v3: what changed for operators, and how gproxy migrate imports a v2 SQLite database into a fresh v3 store"
---

v3 is a rewrite with a new store. A v3 binary does not open a v2 database in
place; instead `gproxy migrate --from-v2` reads the v2 SQLite file read-only
and writes its contents into a v3 store. v2 itself stays maintained on the
`main` branch with `v2.x.y` tags. This page replaces v2's "Migrating From v1
To v2".

## What Changed for Operators

| Area | v2 | v3 |
| --- | --- | --- |
| Configuration | Flags and environment | Flags, environment, `./.env`, `<data-dir>/.env`; no configuration file format. Names are kept where the meaning matched (`GPROXY_HOST`, `GPROXY_PORT`, `GPROXY_DATA_DIR`, `GPROXY_DSN`, `GPROXY_REDIS_URL`, `GPROXY_MASTER_KEY`, `GPROXY_ADMIN_USER`, `GPROXY_ADMIN_PASSWORD`, `UPSTASH_URL`, `UPSTASH_TOKEN`) |
| Persistence | `GPROXY_PERSISTENCE=db` | `sqlite` (default), `libsql`, `postgres`, or `mysql`; `db` is rejected |
| Edge database | `TURSO_URL`, `TURSO_TOKEN` | `GPROXY_LIBSQL_URL`, `GPROXY_LIBSQL_AUTH_TOKEN` |
| First-boot import | `GPROXY_IMPORT_FILE` | Gone; use `gproxy migrate` or the console's config import |
| Web surfaces | `/console` | `/admin` console, `/portal` user portal, `/` public site; APIs under `/admin/api/**` and `/portal/api/**` |
| Container | `latest`, `-musl`, multi-arch; data in `/app/data` | `ghcr.io/leenhawk/gproxy:<tag>` only, linux/amd64; data in `/var/lib/gproxy`; runs as `gproxy` |
| Update channels | `update_channel` instance setting | `releases`, `staging`, `dev`; prerelease builds live on `dev`. The setting is imported |
| Rules | Rewrite rules and message rewrite | One Rules workspace: rule sets attached to providers, routing rules per provider |

See [Configuration](/reference/configuration/), [Container](/deployment/docker/),
and [Routing Rules & Rule Sets](/guides/rules/).

## The migrate Subcommand

```sh
gproxy migrate --from-v2 <path> [--from-v2-master-key <base64>] [--apply] [--merge]
```

| Flag | Meaning |
| --- | --- |
| `--from-v2 <path>` | Path to the v2 SQLite database. Opened read-only; never modified. |
| `--from-v2-master-key <base64>` | The v2 master key (standard base64, 32 bytes), needed only if v2 sealed its secrets. |
| `--apply` | Write the import. Without it the command is a dry run. |
| `--merge` | Allow importing into a v3 store that already contains rows. |

The target store is selected by the normal serving configuration: the same
flags, environment, and `.env` files that `gproxy` uses to start
(`GPROXY_DATA_DIR`, `GPROXY_PERSISTENCE`, `GPROXY_DSN`, `GPROXY_LIBSQL_*`,
`GPROXY_MASTER_KEY`). The target can be SQLite in the data directory or any
other backend.

A dry run reads and decrypts the source, validates it, prints the report,
and writes nothing; it does not open the target at all. With `--apply` the
command opens the target, checks that this source was not imported before,
checks that the target is empty (or `--merge` was given), re-seals every
secret, writes all rows, and records a marker. The process exits non-zero
with `v2 migration was not applied; resolve the reported rows first` when
the report lists problems or nothing was written.

## The Report

```text
v2 migration: dry run
  organizations: 1 importable (1 found)
  users: 3 importable (3 found)
  user_keys: 5 importable (5 found)
  providers: 4 importable (4 found)
  credentials: 6 importable (6 found)
  routes: 8 importable (8 found)
  route_members: 11 importable (11 found)
  ...
  usage: 1520 importable (1520 found)
dry run wrote nothing; rerun with --apply to import
```

Each line names an entity, how many rows the source holds, and how many can
be imported. After `--apply` the lines read `N imported (M found)`. Two
further sections appear when relevant: `existing target rows:` lists what the
target already holds, and `unrecoverable rows:` lists `entity id=N: reason`
for every row that cannot be imported. A source that was already imported
prints `this v2 source was already imported; no rows were written`.

## What Is Imported

| v2 data | v3 result |
| --- | --- |
| `orgs`, `teams`, `users` | Organizations, teams, users. Password hashes are carried over, so administrators and portal users keep their passwords. Each `is_admin` user also receives an allow-all permission. |
| `user_keys` | API keys. The plaintext key is recovered, its v3 digest (SHA-256 of the key without the `sk-`/`at-` prefix, digest version 1) and 12-character prefix are computed, and the key is re-sealed. Clients keep the same keys. |
| `providers` | Providers with the same names, settings, strategy, proxy, and fingerprint. Legacy channel ids are canonicalized: `kimiapi` and `kimicode` become `kimi`; `opencodezen` and `opencodego` become `opencode` with `tier` set to `zen` or `go`. |
| `credentials` | Credentials, decrypted and re-sealed; weight, RPM/TPM limits, proxy, fingerprint, and enabled flag kept. |
| `routes`, `route_members` | Routes (max attempts 6) and members with tier, weight, and upstream model. One exposed model per route, named after the route. |
| `provider_models` | Provider model metadata: display name, variants, context window, max output, thinking flags. |
| `aliases` | Aliases; a `*` provider becomes a global alias, `sort_order` becomes priority. |
| `quotas` | Quotas for organization, team, or user scope with all six windows. |
| `price_rules` | Price rules. `exact` keeps the model name; `contains` becomes the glob `*text*`. `pricing_tiers_json` becomes `tiers`. Exact rules rank first, then longer `contains` patterns. |
| `price_rule_rates` | Dimensional price rates. `cache_read_tokens` is renamed `cached_input_tokens`. A rule with no explicit rates gets seven synthesized per-1,000,000-token rates from its flat v2 columns: input, output, cache read, cache creation 5m/30m/1h, image output. |
| `routing_rules`, `rule_sets`, `rules`, `provider_rule_sets` | The same entities with remapped ids. Legacy `open_ai*` wire-kind ids are normalized to v3 `openai*` ids and compiled during the dry run. |
| `instance_settings` (first row) | Instance name, proxy, usage flag, tokenizer download, upload concurrency, update channel and auto-check, retention days, database size cap, the four log flags, redaction override. `inherit_system_proxy` is set to `false`. |
| `usages` | Usage rows plus hourly rollups recomputed on import. Image-output and cache-creation counters move into `metrics`; `route_name`, `kind`, and `thread_id` are kept as the dimensions `v2_route`, `v2_kind`, `v2_thread_id`. Disabled history-only providers and credentials preserve required references deleted after recording; deleted optional subject ids remain in `v2_deleted_*` dimensions. Usage is written in bounded batches. |

## What Is Not Imported

The importer reads only the tables above. These v2 tables are skipped:

| v2 table | Consequence |
| --- | --- |
| `route_permissions` | No permission rows come across. v3 denies a request when no permission matches the caller. Administrators keep access only because the importer writes them an explicit allow-all permission; every other user, team, or organization needs permissions granted before clients resume. |
| `rate_limits` | Recreate rate limits in the console. |
| `upstream_requests`, `downstream_requests`, `audit_logs` | Request logs and the admin audit trail start fresh. |
| `credential_statuses`, `credential_model_statuses`, `credential_quota_cycles`, `credential_quota_cycle_models`, `credential_usage_daily` | Credential health and quota-cycle state is rebuilt from live traffic. |
| `usage_rollups` | Recomputed from the imported usage rows. |
| `tokenizer_vocabs`, `codex_task_bindings`, `gproxy_kv` | Download vocabularies again; cache and task bindings are transient. |

## Secrets

v2 sealed values are envelopes with `kek_id`, `wrapped_dek`, `nonce`, and
`ciphertext`. The importer never copies ciphertext: every credential secret
and API key is decrypted in memory and re-sealed with the v3 key from
`GPROXY_MASTER_KEY` (AES-256-GCM), or stored in plaintext when v3 runs
without a key. Sealed v2 values need `--from-v2-master-key`; plaintext v2
values need nothing. A secret that cannot be opened is reported under
`unrecoverable rows` and blocks the whole import, so either supply the
right key or remove that row in v2 first.

## Validation and Blockers

Before anything is written the source must be internally consistent: teams
reference an organization, keys a user, credentials and routing rules a
provider, route members a route and a provider, aliases a provider name,
quotas an existing subject, rates a price rule, and rules a rule set. Routing
rules must compile after legacy wire ids are normalized. Usage rows still need
non-null provider and credential ids, valid metrics, and non-negative counters;
references deleted after recording become history tombstones instead of
blocking the import. Weights, limits, and unit sizes must be non-negative. A
price rule whose match type is neither `exact` nor `contains`, or whose pattern
contains `*`, cannot be expressed as a v3 glob. Only one `instance_settings`
row is accepted and its name must not be blank. A row that fails also removes
the rows that reference it, and every removal is listed. Any listed problem
means nothing is written.

Two rules apply to the target:

- **Non-empty targets need `--merge`.** Otherwise the report ends with
  `target store: is not empty; rerun with --merge to combine stores` and
  lists the existing counts. With `--merge`, imported rows receive new ids
  beside the existing ones.
- **Re-import is idempotent.** A successful import records the setting
  `v2_import_<sha256 of the resolved source path>`. Running the command
  again for the same file reports that it was already imported and exits
  zero. The marker keys on the path, so keep importing the same file from
  the same location.

## Procedure

1. Stop v2 so the database is quiescent:
   `systemctl stop gproxy` or `docker stop gproxy`.
2. Copy the database, including `-wal` and `-shm` sidecars if present:

   ```sh
   mkdir -p /srv/v2 && cp data/gproxy.db* /srv/v2/
   ```

3. Decide the v3 target and key. Put the settings where the binary will
   read them at serve time, for example `/var/lib/gproxy/.env`:

   ```sh
   GPROXY_HOST=0.0.0.0
   GPROXY_MASTER_KEY=<standard base64, 32 bytes, optional>
   ```

4. Dry run and read the report:

   ```sh
   gproxy --data-dir /var/lib/gproxy migrate --from-v2 /srv/v2/gproxy.db \
     --from-v2-master-key "$V2_MASTER_KEY"
   ```

5. Apply with the same arguments plus `--apply`.
6. Start v3 with the same data directory and open `/admin`. Log in with
   your v2 administrator credentials; the setup form does not appear because
   the administrator was imported.
7. Verify: Providers show the expected channels and credential counts,
   Routes show members and exposed models, Pricing shows the rules and
   rates, Usage shows the history. Grant permissions, then send a request
   with an existing user key.

A container runs the same subcommand through its entrypoint; see
[Container](/deployment/docker/).

## Rollback

The v2 database is opened read-only and never changed. To go back, stop v3
and start the v2 binary on its own data directory. The v3 store can be
deleted or kept for another attempt; repeating the import into it requires
`--merge` or a fresh target because of the marker.
