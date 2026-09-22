# `gproxy`

The GPROXY v4 command line: the process an operator actually runs.

Everything below this crate is a library that takes configuration as an
argument. This is the layer that reads the environment, opens a file, binds a
socket and prints to a terminal — and it is the only one that does.

- [`gproxy-sdk`](../gproxy-sdk) assembles the engine.
- [`gproxy-app`](../gproxy-app) owns identity, admission and the typed
  operations.
- [`gproxy-host-axum`](../gproxy-host-axum) turns those into an HTTP surface.

```
cargo run -p gproxy -- serve
```

A first start creates the database, the schema, an administrator and one API
key, and prints the two secrets once.

---

## Commands

| Command | What it does |
|---|---|
| `gproxy serve` | Rotate the master key if asked, bootstrap if the instance is new, bind, serve. **The default** — `gproxy` with no subcommand is `gproxy serve`. |
| `gproxy migrate` | Create the schema or apply pending migrations, then exit. |
| `gproxy bootstrap admin` | Create the first administrator. Idempotent. |
| `gproxy export --out <PATH>` | Write this instance's configuration as one JSON document. |
| `gproxy import --in <PATH>` | Replay such a document into this instance. |
| `gproxy service install` | Write this machine's own service unit, enable it and start it. |

Every configuration flag is global: `gproxy --port 9000 serve` and
`gproxy serve --port 9000` are the same invocation.

### `serve`

```
gproxy serve --host 0.0.0.0 --port 7070 --data-dir /var/lib/gproxy
```

Binds, serves, and stops on `SIGINT` or `SIGTERM` — draining in-flight requests
first. There is no shutdown timeout: a streamed completion legitimately runs for
minutes, and a supervisor that wants a deadline has `TimeoutStopSec`.

The bind happens **after** the schema work, so a process that is still migrating
refuses connections outright rather than accepting them into a backlog nothing
is answering.

### `migrate`

```
gproxy migrate --dsn postgres://gproxy@db/gproxy
```

`serve` runs the same SeaORM migrations. Empty databases receive the current
schema and a `seaql_migrations` ledger; existing databases apply pending entries.
Tables without that ledger, or a ledger newer than this build, are refused before
DDL. `gproxy migrate --status` reads migration status without creating tables.
 The separate command exists for a
deployment that runs migrations as their own step, with one writer, before any
instance starts — which is what more than one instance over one database
requires.

### `bootstrap admin`

```
gproxy bootstrap admin --user admin --password "…" --api-key "sk-…"
```

`--user`, `--password` and `--api-key` are aliases for the global
`--admin-user`, `--admin-password` and `--admin-api-key`, so the same values
work on `serve`.

### Automatic v3 upgrade

Start with the same data directory and `GPROXY_MASTER_KEY`. v4 recognizes the
v3 SQLite database, imports a snapshot beside it, and replaces the original path
only after success, keeping a `gproxy.db.v3-*.bak` backup. Passwords and API keys
remain valid. `gproxy migrate` follows the same path.

`import --from-v3` remains available for separately importing a backup or JSON;
it is not required for a normal upgrade. See the
[migration guide](../../docs/src/content/docs/deployment/v3-to-v4.md).

### `export` / `import`

```
gproxy export --out config.json --include-secrets
gproxy import --in config.json --mode merge --source-master-key "…"
```

`-` means standard output or standard input.

What travels is the configuration a deployment *is*: connection profiles,
providers, credentials, the model catalog, routing, operation overrides, rewrite
rules, quotas, pricing and the settings row. **Identity does not travel** —
users, keys, organizations, teams, permissions and subscriptions belong to the
application layer — so an imported instance still needs its own bootstrap. Usage
and captures do not travel either: copying them would fabricate history the
destination never had.

`--mode merge` writes what the document names and leaves the rest alone.
`--mode replace` additionally deletes rows of an exported kind that the document
does not mention.

With `--include-secrets` the sealed credential blobs travel base64-encoded,
never opened and never plaintext — the document is then exactly as sensitive as
the database file. On the way back in:

| The importer has | What happens |
|---|---|
| `--source-master-key` | every secret is opened and re-sealed under this instance's key |
| the same key as the source | the blob is stored verbatim and already opens |
| neither | the credential is skipped, counted and warned about |

### `service`

```
gproxy service install [--autostart] [--port …] [--data-dir …]
gproxy service uninstall
gproxy service status
```

**There is no `--daemon`, no pidfile and no `gproxy stop`.** Backgrounding,
restart-on-crash and start-at-boot are three problems every operating system
already solved, and a built-in daemon mode would mean owning a pidfile that
goes stale, a log nobody rotates, a restart loop with no backoff and a stop
command that races whatever else holds the port. `gproxy serve` stays a
foreground process that exits on `SIGTERM`; `gproxy service install` writes the
unit that a supervisor built for the job reads.

The unit reproduces **this** invocation: `gproxy service install --port 9000
--data-dir /srv/gproxy` and `gproxy serve --port 9000 --data-dir /srv/gproxy`
are the same instance. Relative paths are made absolute first, because the unit
runs with a working directory the init system chose.

| Platform | What is written | Started by |
|---|---|---|
| Linux + systemd | `~/.config/systemd/user/gproxy.service` | `systemctl --user enable --now`, plus `loginctl enable-linger` with `--autostart` |
| macOS | `~/Library/LaunchAgents/io.github.leenhawk.gproxy.plist` | `launchctl bootstrap gui/<uid>` |
| Windows | a logon-triggered task named `gproxy` | `schtasks /create /xml` |
| Termux | `~/.termux/boot/gproxy.sh` | the **Termux:Boot** add-on, which must be installed separately |

A machine with none of those — a container, a chroot, a distribution on runit
or s6, an Android app that is not Termux — is told so, with the reason, instead
of being handed a unit nothing will read.

#### What the unit never contains

**A secret.** `~/.config/systemd/user/gproxy.service` is a plain file a
directory away from the database it would unlock, it survives every backup of
`$HOME`, and `systemctl --user show` reads it back to anyone who asks. So the
unit carries the environment file's **path** and never a value out of it:
`EnvironmentFile=-` on systemd, `GPROXY_ENV_FILE` on the other three.

`install` says which of the three cases it found:

| Where the master key came from | What `install` does |
|---|---|
| the environment file gproxy reads | names the file; the key stays in it |
| an exported variable, or `--master-key` | **refuses to copy it**, and names the file to put it in |
| nowhere | says the service will store secrets unencrypted |

Only `--host`, `--port`, `--data-dir` and `--config` reach the command line in
the unit. Everything else — a DSN with a password in it, a Redis URL, an admin
password — belongs in the environment file or the config file, which are files
an operator can give a mode to.

#### `--autostart`

Installing always gives you a service that starts when you log in.
`--autostart` asks for the other thing: running with nobody logged in.

- **systemd** — `loginctl enable-linger`. `uninstall` turns it back off *only*
  if `install --autostart` was what turned it on, which the unit records in a
  comment of its own.
- **macOS** — not possible for a `LaunchAgent`, and nothing is done about it. A
  boot-time service is a root `LaunchDaemon`, and a gateway that writes its
  database as root is worse than one that waits for a login.
- **Windows** — not possible without elevation. A task that starts at boot runs
  as SYSTEM or stores your password in the task store.
- **Termux** — already the case. Termux:Boot is a boot hook and nothing else.

`status` reports what the init system says — loaded, active, enabled, the last
exit, the restart count — not what this command hopes:

```
$ gproxy service status
gproxy, as the systemd user manager sees it.
  unit       /home/leen/.config/systemd/user/gproxy.service
  loaded     loaded
  active     inactive
  sub-state  dead
  enabled    enabled
  restarts   1
  result     success
  last exit  exited with status 0
  linger     yes
```

---

## Configuration

Five sources, strongest first:

1. **the command line** — `--port 9000`
2. **the real environment** — `GPROXY_PORT=9000`
3. **`.env`** — loaded without overriding anything the environment already set
4. **the TOML file** named by `--config`
5. **the built-in defaults**

A flag beats an environment variable, an environment variable beats `.env`, and
`.env` beats the file. Putting the file *under* the environment is the choice
that matters: a file is a deployment's checked-in intent, and the environment is
how one host or one container deviates from it. If the file won, a `GPROXY_PORT`
in a compose file would silently do nothing.

The layering happens exactly once, at startup. No module below the entry point
reads `std::env`.

### Environment

Every value is one `clap` field carrying its `env = …`, so `--help` prints the
environment variable next to the flag it shadows and this table cannot drift
away from the program. Names that existed in v3 mean what they meant there: an
upgrade is not a redeployment.

| Variable | Flag | Default | What it is |
|---|---|---|---|
| `GPROXY_CONFIG` | `--config`, `-c` | — | TOML file holding any `AppConfig` field |
| `GPROXY_HOST` | `--host` | `127.0.0.1` | listen address (an IP, not a hostname) |
| `GPROXY_PORT` | `--port`, `-p` | `7070` | listen port |
| `GPROXY_DATA_DIR` | `--data-dir` | `data` | root that relative paths resolve against |
| `GPROXY_PERSISTENCE` | `--persistence` | `sqlite` | `sqlite`, `postgres` or `mysql` (`db` is v2's name for `sqlite`) |
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
| `GPROXY_AUTOSTART` | `--autostart` | `false` | `service install` only: also run without a login session |
| `GPROXY_ENV_FILE` | — | `.env` | which `.env` to load |

`GPROXY_ENV_FILE` has no flag on purpose: a flag would have to be parsed by the
very step it feeds, so it could not affect the values `clap` itself reads.

Booleans take `1`, `true`, `yes`, `on`, `0`, `false`, `no` or `off`. A
misspelling is an error rather than a `false` — the two values this gates,
rotating every secret in the database and serving the console, are both things
whose absence goes unnoticed until it matters.

### The config file

The file speaks `AppConfig`'s own field names, which are `snake_case`. Unknown
keys are an error rather than a silent no-op, so a typo is reported at startup.

```toml
host = "0.0.0.0"
port = 7070
data_dir = "/var/lib/gproxy"
public_base_url = "https://gproxy.example.com"
cors_origins = ["https://console.example.com"]
trusted_proxies = ["10.0.0.0/8"]
session_ttl_secs = 2592000

[store]
kind = "sqlite"
path = "gproxy.db"
# kind = "url"
# dsn = "postgres://gproxy@db/gproxy"

[cache]
kind = "memory"
# kind = "redis"
# url = "redis://cache:6379"
# namespace = "prod"

[master_key]
rotate = false

[master_key.key]
kind = "hex"
value = "0000000000000000000000000000000000000000000000000000000000000000"

[file_storage]
kind = "fs"
root = "files"
# kind = "s3"
# bucket = "gproxy"
# region = "auto"
# endpoint = "https://…"

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

---

## Secrets

Credential secrets — and retained API keys, and the tokenizer auth token — are
sealed at rest with AES-256-GCM under `GPROXY_MASTER_KEY`. The seal is bound to
the row's own id, so a blob copied onto another row does not open.

**With no master key, secrets are stored unencrypted.** That is a supported
deployment, not an accident, and the binary says so loudly once at startup,
naming the variable that would fix it:

```
WARN upstream credential secrets are stored UNENCRYPTED: no master key is
     configured. Set GPROXY_MASTER_KEY to 32 bytes as 64 hex characters or
     base64 …
```

### Rotation

Changing the key is not a configuration edit: every sealed blob has to be opened
with the old key and sealed again with the new one, or the next reload fails on
the first credential it cannot open. So rotation is a real operation, performed
at startup:

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

- **One transaction.** Every `UPDATE` and the revision bump commit together, so
  a failure leaves the database entirely on the old key and the rotation can
  simply be retried. There is never a half-rotated database to work out the
  shape of first.
- **Everything is opened before anything is written.** A key that decrypts most
  of the rows aborts before the first write.
- **Nothing is skipped.** A blob that cannot be opened is an error, never a row
  left behind — a skipped row would be unopenable by *both* keys once the
  operator promotes the new one.

Two rules follow from the design:

- **Rotate with one instance running.** There is no durable record of which key
  the database is on, so a peer still holding the old key would see the revision
  bump, reload, and fail to open anything.
- **The rotating process serves on the new key** for the rest of its life,
  because that is what the database now holds. Forgetting step 4 means the next
  start cannot open anything — loudly, at assembly, not silently.

`GPROXY_MASTER_KEY_NEXT` with an unset `GPROXY_MASTER_KEY` is the adoption
path: it seals a database that was running in plaintext.

Setting `GPROXY_MASTER_KEY_NEXT` **without** `GPROXY_MASTER_KEY_ROTATE` does
nothing and says so, so a next key sitting in a deployment for a month is not
mistaken for a rotation that happened.

---

## Bootstrap

A fresh database has no way in: the console needs a user and the admin API needs
a key. So the first start creates one administrator, mints one gateway API key,
and prints once, to standard output:

```
GPROXY first-run administrator (shown once)
  user:     admin
  password: eUGXB4mnDhWmoYrDafattTFVjoR3SIyE
  api key:  sk-9uzmySzNRy0uT0ERTTrKNbfv5DeMkoXseGb0dULT19s
Save these before closing this terminal; they are not stored in a form
this instance can show you again.
```

- The trigger is an **empty `users` table**. Any user at all means the instance
  has been set up, and nothing is touched: no password is reset, no key is
  minted, no row is changed. An operator restarting a container that still
  carries `GPROXY_ADMIN_PASSWORD` is not asking for a password reset.
- **Only what the operator does not already know is printed.** Supply
  `GPROXY_ADMIN_PASSWORD` and it is used but not echoed; supply
  `GPROXY_BOOTSTRAP_ADMIN_API_KEY` and that exact key is minted, not a new one.
- The output goes to **stdout via `println!`, never to the log**, so the secrets
  do not land in a journal or whatever ships it somewhere else. Log lines go to
  stderr for the same reason, which is also what keeps `export --out -` clean.
- Every write goes through `gproxy-app`'s own `Operations` families, so the
  password is validated and argon2-hashed by the product's rules and the key is
  digested by the product's function. In v3 the bootstrap key was written with
  hand-rolled SQL under a digest the admin API did not look it up by, and an
  `sk-`-prefixed bootstrap key answered `401` on every request. The fix is not a
  better digest — it is having only one.

---

## The console

`/console` is served from a bundle compiled into the binary, or from the
directory named by `GPROXY_CONSOLE_PATH`. **A source checkout embeds nothing**,
and that is the intended state: `cargo run` has no console in it, so every
console path answers 404 rather than a blank page that looks like a broken
application. The startup log says so.

The bundle lives in `gproxy-host-axum/assets/web`, which a release build fills
from the console's own `dist` before `cargo build`. Nothing in this crate
participates in the embedding — the seam is `rust-embed` in
[`gproxy_host_axum::console`](../gproxy-host-axum/src/console.rs), and it is
already in place for the console to land in.

---

## Cargo features

| Feature | Default | What it adds |
|---|---|---|
| `channels` | ✓ | every channel this repository implements. Name channels one by one (`codex`, `kiro`, `openai`, …) for a binary that carries only the upstreams a deployment uses |
| `memory` | ✓ | the in-process cache |
| `fs` | ✓ | local file storage |
| `bundled-vocabulary` | ✓ | a fallback tokenizer vocabulary |
| `postgres` | | the PostgreSQL driver |
| `mysql` | | the MySQL driver |
| `redis` | | the shared cache a multi-instance deployment needs |
| `s3` | | S3-compatible file storage |

SQLite is always compiled in. A backend this build does not have is refused at
startup with the feature name that would provide it, rather than at the first
request.

---

## Running more than one instance

Three things change:

1. **`GPROXY_REDIS_URL`.** The default cache is process-local, so two instances
   would not see each other's invalidations and would each count rate limits on
   their own.
2. **`gproxy migrate` as its own step**, with one writer, before any instance
   starts.
3. **Rotate with one instance running**, as above.
