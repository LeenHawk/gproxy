---
title: "Configuration"
description: "The five configuration sources and their order, all 24 GPROXY_* variables, the TOML file, master-key rotation, bootstrap, and the runtime settings row."
---

GPROXY reads its process configuration **once, at startup**. No module below
the entry point reads the environment.

Everything that changes while the process runs — providers, credentials,
routes, rewrite rules, pricing, identity, and the settings row at the end of
this page — lives in the database.

## The Five Sources

Strongest first:

1. **the command line** — `--port 9000`
2. **the real environment** — `GPROXY_PORT=9000`
3. **`.env`**, loaded without overriding anything the environment already set
4. **the TOML file** named by `--config`
5. **the built-in defaults**

Putting the file *under* the environment is the choice that matters. A file is
a deployment's checked-in intent; the environment is how one host or one
container deviates from it. If the file won, a `GPROXY_PORT` in a compose file
would silently do nothing.

Every configuration flag is global, so `gproxy --port 9000 serve` and
`gproxy serve --port 9000` are the same invocation.

## The Environment

Every value is one flag carrying its variable name, so `gproxy --help` prints
the variable next to the flag it shadows and this table cannot drift away from
the program. Names that existed in v3 mean what they meant there: an upgrade is
not a redeployment.

| Variable | Flag | Default | What it is |
| --- | --- | --- | --- |
| `GPROXY_CONFIG` | `--config`, `-c` | — | TOML file holding any config field |
| `GPROXY_HOST` | `--host` | `127.0.0.1` | listen address (an IP, not a hostname) |
| `GPROXY_PORT` | `--port`, `-p` | `7070` | listen port |
| `GPROXY_DATA_DIR` | `--data-dir` | `data` | root that relative paths resolve against |
| `GPROXY_PERSISTENCE` | `--persistence` | `sqlite` | `sqlite`, `postgres` or `mysql` |
| `GPROXY_DSN` | `--dsn` | — | connection string; names its own backend when `--persistence` is absent |
| `GPROXY_REDIS_URL` | `--redis-url` | — | shared cache; **required for more than one instance** |
| `GPROXY_MASTER_KEY` | `--master-key` | — | 32 bytes as 64 hex characters or base64. Unset stores secrets in plaintext |
| `GPROXY_MASTER_KEY_NEXT` | `--master-key-next` | — | the key to re-seal to |
| `GPROXY_MASTER_KEY_ROTATE` | `--master-key-rotate` | `false` | perform the rotation at startup |
| `GPROXY_PUBLIC_BASE_URL` | `--public-base-url` | — | external origin, for publication links and the OAuth issuer identifier |
| `GPROXY_CORS_ORIGINS` | `--cors-origin` | — | comma-separated browser origins; empty means same-origin only |
| `GPROXY_TRUSTED_PROXIES` | `--trusted-proxy` | — | comma-separated peers whose `x-forwarded-*` is believed; empty trusts nothing |
| `GPROXY_FILE_STORAGE_DIR` | `--file-storage-dir` | — | local directory for published bodies and vocabularies |
| `GPROXY_CONSOLE` | `--console` | `true` | serve the console |
| `GPROXY_CONSOLE_PATH` | `--console-path` | — | serve it from this directory instead of the embedded bundle |
| `GPROXY_INSTANCE_ID` | `--instance-id` | random | a stable name for this process |
| `GPROXY_LOG_FORMAT` | `--log-format` | `text` | `text` or `json` |
| `GPROXY_LOG_FILTER` | `--log-filter` | `RUST_LOG`, else `info` | tracing filter |
| `GPROXY_ADMIN_USER` | `--admin-user` / `--user` | `admin` | the first administrator's name |
| `GPROXY_ADMIN_PASSWORD` | `--admin-password` / `--password` | generated | their password |
| `GPROXY_BOOTSTRAP_ADMIN_API_KEY` | `--admin-api-key` / `--api-key` | generated | the exact API key to mint for them |
| `GPROXY_IMPORT_SOURCE_MASTER_KEY` | `--source-master-key` | — | `import` only: the source instance's key |
| `GPROXY_ENV_FILE` | — | `.env` | which `.env` to load |

`GPROXY_ENV_FILE` has no flag on purpose: a flag would have to be parsed by the
very step it feeds, so it could not affect the values the parser itself reads.

Booleans take `1`, `true`, `yes`, `on`, `0`, `false`, `no` or `off`. **A
misspelling is an error rather than a `false`** — the two values this gates,
rotating every secret in the database and serving the console, are both things
whose absence goes unnoticed until it matters.

## The Commands

| Command | What it does |
| --- | --- |
| `gproxy serve` | Rotate the master key if asked, bootstrap if the instance is new, bind, serve. **The default** — `gproxy` with no subcommand is `gproxy serve`. |
| `gproxy migrate` | Create or incrementally synchronize the schema, then exit. |
| `gproxy bootstrap admin` | Create the first administrator. Idempotent. |
| `gproxy export --out <PATH>` | Write this instance's configuration as one JSON document. |
| `gproxy import --in <PATH>` | Replay such a document into this instance. |

`serve` stops on `SIGINT` or `SIGTERM`, draining in-flight requests first.
**There is no shutdown timeout**: a streamed completion legitimately runs for
minutes, and a supervisor that wants a deadline has one. The bind happens
**after** the schema work, so a process that is still migrating refuses
connections outright rather than accepting them into a backlog nothing is
answering.

## The Config File

The file speaks the configuration type's own field names, which are
`snake_case`. **Unknown keys are an error**, so a typo is reported at startup
rather than silently ignored.

```toml
host = "0.0.0.0"
port = 7070
data_dir = "/var/lib/gproxy"
public_base_url = "https://gproxy.example.com"
cors_origins = ["https://console.example.com"]
trusted_proxies = ["10.0.0.0/8"]
session_ttl_secs = 2592000

[store]              # kind = "url" with a dsn for postgres or mysql
kind = "sqlite"
path = "gproxy.db"

[cache]              # kind = "redis" with url and namespace, for several instances
kind = "memory"

[master_key]
rotate = false

[master_key.key]
kind = "hex"
value = "0000000000000000000000000000000000000000000000000000000000000000"

[file_storage]       # kind = "s3" with bucket, region and endpoint
kind = "fs"
root = "files"

[console]
enabled = true

[oauth]
access_ttl_secs = 3600
refresh_ttl_secs = 2592000
code_ttl_secs = 300
device_ttl_secs = 900
cli_client_ids = []
```

Several fields have no flag because they are not things a container overrides:
`session_ttl_secs`, the whole `[oauth]` block, and the S3 details.

This is the **same document** the Workers host reads as JSON in `GPROXY_CONFIG`
and the desktop shell reads as `gproxy.toml` in its data directory, so there is
no second schema to keep in step.

## Secrets at Rest

Credential secrets, retained API keys and the tokenizer source token are sealed
with AES-256-GCM under `GPROXY_MASTER_KEY`. **The seal is bound to the row's
own id**, so a blob copied onto another row does not open.

With no master key, secrets are stored unencrypted. That is a supported
deployment, not an accident, and the binary warns loudly once at startup,
naming the variable that would fix it and the two that would seal a database
already holding secrets.

Generate one with `openssl rand -hex 32`, which is exactly 64 hex characters. A
32-byte base64 key is 43 or 44 characters, so the two encodings cannot be
confused and the format is sniffed rather than configured.

### Master-Key Rotation

Changing the key is not a configuration edit. Every sealed blob has to be
opened with the old key and sealed again with the new one, or the next reload
fails on the first credential it cannot open. So rotation is a real operation,
performed at startup:

```sh
# 1. Stop every instance.
# 2. Start one with the rotation armed.
GPROXY_MASTER_KEY=<old> \
GPROXY_MASTER_KEY_NEXT=<new> \
GPROXY_MASTER_KEY_ROTATE=true \
  gproxy serve

# 3. It logs, at WARN:
#    master key rotated; copy GPROXY_MASTER_KEY_NEXT to GPROXY_MASTER_KEY,
#    then clear GPROXY_MASTER_KEY_NEXT and GPROXY_MASTER_KEY_ROTATE

# 4. Promote the key and clear the other two.
GPROXY_MASTER_KEY=<new> gproxy serve
```

Three properties make it safe to run:

- **One transaction.** Every update and the revision bump commit together, so a
  failure leaves the database entirely on the old key and the rotation can
  simply be retried. There is never a half-rotated database to work out the
  shape of first.
- **Everything is opened before anything is written.** A key that decrypts most
  of the rows aborts before the first write.
- **Nothing is skipped.** A blob that cannot be opened is an error, never a row
  left behind — a skipped row would be unopenable by *both* keys once the
  operator promotes the new one.

Two rules follow from the design:

- **Rotate with one instance running.** There is no durable record of which key
  the database is on, so a peer still holding the old key would see the
  revision bump, reload, and fail to open anything.
- **The rotating process serves on the new key** for the rest of its life,
  because that is what the database now holds. Forgetting step 4 means the next
  start cannot open anything — loudly, at assembly, not silently.

`GPROXY_MASTER_KEY_NEXT` with an unset `GPROXY_MASTER_KEY` is the **adoption
path**: it seals a database that was running in plaintext.

Setting `GPROXY_MASTER_KEY_NEXT` *without* `GPROXY_MASTER_KEY_ROTATE` does
nothing and says so, so a next key sitting in a deployment for a month is not
mistaken for a rotation that happened.

## Bootstrap

A fresh database has no way in, so the first start creates one administrator,
mints one gateway API key and prints both once, to standard output. See
[Installation](/getting-started/installation/#run-it) for what that looks like.

- **The trigger is an empty users table.** Any user at all means the instance
  has been set up: no password is reset, no key is minted, no row is changed.
  An operator restarting a container that still carries
  `GPROXY_ADMIN_PASSWORD` is not asking for a password reset.
- **Only what the operator does not already know is printed.** Supply
  `GPROXY_ADMIN_PASSWORD` and it is used but not echoed; supply
  `GPROXY_BOOTSTRAP_ADMIN_API_KEY` and that exact key is minted.
- **Every write goes through the product's own operation families**, so the
  password is validated and hashed by the product's rules and the key is
  digested by the product's function. In v3 the bootstrap key was written with
  hand-rolled SQL under a digest the admin API did not look it up by, and an
  `sk-` prefixed bootstrap key answered `401` on every request. The fix is not
  a better digest — it is having only one.

## Moving a Configuration

```sh
gproxy export --out config.json --include-secrets
gproxy import --in  config.json --mode merge --source-master-key '…'
```

`-` means standard output or standard input. The document is the management
DTOs themselves, so an export says exactly what a listing would have said, in
replay order — no row appears before the row it points at:

```json
{"formatVersion":4,"exportedAtMs":1789981723118,"secretsOmitted":true,
 "secrets":[],"data":{"connectionProfiles":[],"providers":[…],"credentials":[…],
 "models":[],"providerModels":[],"routes":[],"routeMembers":[],
 "exposedModels":[],"operationRules":[],"operationEndpoints":[],
 "rewriteRuleSets":[],"rewriteRules":[],"providerRewriteRuleSets":[],
 "quotas":[],"priceRules":[],"priceRates":[],"priceTiers":[],"settings":null}}
```

**Identity does not travel** — users, keys, organizations, teams, permissions,
subscriptions and OAuth clients belong to the product layer — so an imported
instance still needs its own bootstrap. Usage and captures do not travel
either: copying them would fabricate history the destination never had.

`--mode merge` writes what the document names and leaves the rest alone;
`--mode replace` additionally deletes rows of an exported kind the document
omits, children before parents, and never touches an identity, usage or capture
table. An import is **one revision commit for the whole document**, so a
document refused anywhere leaves nothing behind.

With `--include-secrets` the sealed blobs travel base64-encoded, never opened
and never plaintext — the document is then exactly as sensitive as the database
file. On the way back in:

| The importer has | What happens |
| --- | --- |
| `--source-master-key` | every secret is opened and re-sealed under this instance's key |
| the same key as the source | the blob is stored verbatim and already opens |
| neither | the credential is skipped, counted and warned about |

That rule exists because the engine opens every credential's secret while it
assembles a snapshot: a credential imported unopenable would not break its own
calls, it would break **every later reload of the whole instance**.

## The Settings Row

Runtime settings live in the database and take effect without a restart.

```sh
curl -s http://127.0.0.1:7070/admin/api/settings  -H "Authorization: Bearer $GPROXY_KEY"
curl -s -X PATCH http://127.0.0.1:7070/admin/api/settings \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"instance":{"maxAttempts":4}}'
```

One row, two groups. The instance group:

| Key | Default | Meaning |
| --- | --- | --- |
| `instanceName` | `default` | recorded with usage |
| `maxAttempts` | `6` | hard ceiling on a plan's upstream attempts; a route's own budget is capped by it |
| `maxInFlight` | `1024` | concurrent requests |
| `requestTimeoutMs` | `600000` | whole-request deadline |
| `streamIdleTimeoutMs` | `60000` | gap between stream units |
| `maxRequestBodyBytes` | `67108864` | a streaming body over this stays a stream, and the plan is cut to one target |
| `maxResponseBodyBytes` | `67108864` | buffered response cap |
| `maxStreamEventBytes` | `1048576` | one SSE event or array element |
| `maxWsFrameBytes` | `16777216` | a larger frame closes both sides with `1009` |
| `maxMultipartParts` | `64` | |
| `enableSettlement`, `enableUsage` | `true` | pricing and the usage row |
| `enableTokenizerVocabs` | `true` | count with a real vocabulary |
| `enableTokenizerDownload` | `false` | fetch an uncached vocabulary |
| `retentionDays`, `maxDatabaseSizeMb` | unset | |
| `portalRecentRequestsEnabled` | `true` | whether the portal shows recent requests |
| `corsOrigins`, `trustedProxies`, `connectionProfileId`, `oauthClientAllowlist` | | the database's copy of the same policies |
| `configRevision` | | read-only: the revision every instance synchronizes on |

The logging group is `enableDownstreamLog`, `enableDownstreamLogBody`,
`enableUpstreamLog`, `enableUpstreamLogBody`, `disableLogRedaction`,
`enableTracing`, `logLevel`, `logFormat` and three blacklists. The two body
switches are off by default; see
[Usage, Logs & Audit](/guides/observability/#captures).

## Running More Than One Instance

Three things change. **`GPROXY_REDIS_URL`**, because the default cache is
process-local and two instances would not see each other's invalidations and
would each count rate limits on their own — and a cache that cannot answer
*refuses* a rate-limited request rather than passing it, because a silent local
fallback turns an outage into "every limit on the instance is off".
**`gproxy migrate` as its own step**, with one writer, before any instance
starts. And **rotate with one instance running**, as above. See
[Storage & Cache Backends](/reference/database/#the-cache).

## The Trusted-Proxy Rule

`x-forwarded-for` and `x-forwarded-proto` are headers, and a header is whatever
the peer wrote. They are believed **only when the socket's peer is loopback or
is listed in `trustedProxies`**. From any other peer they are ignored outright
— not merged, not preferred, not used as a fallback — and the default trusts
nothing.

Both matter, for different reasons. A forged `x-forwarded-for` picks the
address in the operator's log next to somebody's request. A forged
`x-forwarded-proto` picks the **scheme of the OAuth issuer identifier**, which
is a discovery document telling a client where to send an authorization code.

When the peer is unknown it is treated as untrusted.
