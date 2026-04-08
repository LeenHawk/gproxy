# Deployment Guide

### Build

Single-instance release build:

```bash
cargo build -p gproxy --release
```

If you changed the embedded console frontend, build it before packaging or running the binary:

```bash
cd frontend/console
pnpm install
pnpm build
```

Multi-instance build with the Redis backend enabled:

```bash
cargo build -p gproxy --release --features redis
```

The output binary is located at `target/release/gproxy`.

### Embedded Console

The current binary includes an embedded browser console mounted at `/console`.

- Console URL: `http://127.0.0.1:8787/console`
- Browser login: `POST /login`
- Browser auth header: `Authorization: Bearer <session_token>`

Typical local workflow:

```bash
cd frontend/console
pnpm install
pnpm build

cargo run -p gproxy
```

Then open `/console` in a browser and log in with a current v1 username and password.

### Environment Variables

The full set of startup parameters and their corresponding environment variables are defined in `apps/gproxy/src/main.rs`:

| Environment Variable | Default | Required | Description |
| --- | --- | --- | --- |
| `GPROXY_HOST` | `127.0.0.1` | No | Listen address. |
| `GPROXY_PORT` | `8787` | No | Listen port. |
| `GPROXY_ADMIN_USER` | `admin` | No | Bootstrap admin username used when creating or reconciling the admin account. |
| `GPROXY_ADMIN_PASSWORD` | None | No | Bootstrap admin password. On first startup, if an admin account must be created and no password is provided, one is generated and logged once. |
| `GPROXY_ADMIN_API_KEY` | None | No | Bootstrap admin API key. On first startup, if an admin account must be created and no API key is provided, one is generated and logged once. |
| `GPROXY_DSN` | If unset, `sqlite://<data_dir>/gproxy.db?mode=rwc` is generated automatically. | No | Database DSN. |
| `GPROXY_PROXY` | None | No | Upstream HTTP proxy. |
| `GPROXY_SPOOF` | `chrome_136` | No | TLS fingerprint emulation name. |
| `DATABASE_SECRET_KEY` | None | No | Database-at-rest encryption key; when set, credentials, passwords, and API keys are encrypted at rest with XChaCha20Poly1305. |
| `GPROXY_REDIS_URL` | None | No | Redis DSN; the Redis backend is enabled only when the binary is built with the `redis` feature. |
| `GPROXY_CONFIG` | `gproxy.toml` | No | TOML config path used as the seed file during first-time initialization. |
| `GPROXY_DATA_DIR` | `./data` | No | Data directory; the default SQLite file and runtime data are based on this directory. |

Additional Notes:

- CLI arguments and environment variables are both parsed by `clap`; explicit CLI values take priority over defaults.
- If the database already contains `global_settings` and `GPROXY_DSN` / `GPROXY_DATA_DIR` were not passed explicitly at startup, the process will reconnect to the database using the persisted configuration.

### TOML Config Format

The TOML file pointed to by `GPROXY_CONFIG` is only used during initialization when the database does not already contain data. The corresponding structure is defined in `crates/gproxy-api/src/admin/config_toml.rs`.

```toml
[global]
host = "0.0.0.0"
port = 8787
proxy = "http://127.0.0.1:7890"
spoof_emulation = "chrome_136"
update_source = "github"
enable_usage = true
enable_upstream_log = false
enable_upstream_log_body = false
enable_downstream_log = false
enable_downstream_log_body = false
dsn = "sqlite://./data/gproxy.db?mode=rwc"
data_dir = "./data"

[[providers]]
name = "openai-main"
channel = "openai"
settings = { base_url = "https://api.openai.com/v1" }
credentials = [
  { api_key = "sk-provider-1" }
]

[[models]]
provider_name = "openai-main"
model_id = "gpt-4.1-mini"
display_name = "GPT-4.1 mini"
enabled = true
price_each_call = 0.0

[[model_aliases]]
alias = "chat-default"
provider_name = "openai-main"
model_id = "gpt-4.1-mini"
enabled = true

[[users]]
name = "alice"
password = "plain-text-or-argon2-phc"
enabled = true

[[users.keys]]
api_key = "sk-user-1"
label = "default"
enabled = true

[[permissions]]
user_name = "alice"
provider_name = "openai-main"
model_pattern = "gpt-*"

[[file_permissions]]
user_name = "alice"
provider_name = "openai-main"

[[rate_limits]]
user_name = "alice"
model_pattern = "gpt-*"
rpm = 60
rpd = 10000
total_tokens = 200000

[[quotas]]
user_name = "alice"
quota = 100.0
cost_used = 0.0
```

Field Descriptions:

- `[global]` covers global listen address, logging, update source, DSN, and data directory configuration.
- `[[providers]]` defines a provider; `settings` and `credentials` are both JSON values read via `serde_json::Value`.
- `[[models]]` / `[[model_aliases]]` define forwardable models and their aliases.
- Admin access is represented by `[[users]]` entries with `is_admin = true` and at least one enabled `[[users.keys]]` entry. If the seed config does not define such an admin, startup can bootstrap one from `GPROXY_ADMIN_USER`, `GPROXY_ADMIN_PASSWORD`, and `GPROXY_ADMIN_API_KEY`.
- The `password` field under `[[users]]` can be either plaintext or a direct Argon2 PHC hash.
- `[[users.keys]]` is a nested array table representing the user's API key list.
- `[[permissions]]`, `[[file_permissions]]`, `[[rate_limits]]`, and `[[quotas]]` correspond to model permissions, file permissions, rate limiting, and cost quotas respectively.

### Database Support

`gproxy-storage` compiles in three database backends via SeaORM / SQLx:

| Database | DSN Prefix | Description |
| --- | --- | --- |
| SQLite | `sqlite:` | Default mode; if `GPROXY_DSN` is not set explicitly, startup generates a SQLite file DSN automatically. |
| PostgreSQL | `postgres:` | Provided by `sqlx-postgres` and the SeaORM Postgres feature. |
| MySQL | `mysql:` | Provided by `sqlx-mysql` and the SeaORM MySQL feature. |

Common DSN examples:

```text
sqlite://./data/gproxy.db?mode=rwc
postgres://gproxy:secret@127.0.0.1:5432/gproxy
mysql://gproxy:secret@127.0.0.1:3306/gproxy
```

After establishing the connection, `SeaOrmStorage::connect()` will:

1. Optionally load the database encryptor corresponding to `DATABASE_SECRET_KEY`.
2. Apply per-database connection tuning parameters.
3. Connect to the database and run `sync()` to synchronize the schema.

### Multi-Instance Deployment

#### Basic Approach

The repository supports multi-instance deployment with a shared database and an optional shared Redis backend:

1. Use the same database DSN across all instances.
2. Build all instances with `--features redis`.
3. Set the same `GPROXY_REDIS_URL` on all instances.

#### Current Redis Backend Coverage

Based on the actual injection logic in `apps/gproxy/src/main.rs`, the current binary enables at runtime:

- `RedisQuota`
- `RedisRateLimit`

Although `RedisAffinity` is also implemented in `gproxy-core`, the current `apps/gproxy` binary does not inject it into `AppStateBuilder`, so it is not an enabled shared backend at this time.

#### What Is Shared vs. Local

Shared state:

- Persisted data in the database: `global_settings`, providers, credentials, models, aliases, users, keys, permissions, file permissions, usages, request logs, credential statuses, user file records, etc.
- Runtime data in Redis: user quota reservation / settle state, and rate-limit counting windows.

Local state:

- Each process's own `AppState` in-memory snapshot: Identity, Policy, Routing, File, Config, QuotaService.
- SDK engine, upstream connection pool, worker buffers, shutdown signals, logging context.
- Rate-limit counters when Redis is not enabled.

Synchronization behavior inferred from source:

- Caches for providers, models, aliases, users, keys, permissions, and file permissions are primarily refreshed on startup or via `POST /admin/reload`.
- Quota additionally has a `QuotaReconciler` that runs every 30 seconds, reconciling database quotas back into local memory.
- Therefore, in a multi-instance scenario, if you change admin configuration on one instance, other instances typically need to execute `/admin/reload` or restart to see the updated cache immediately; quota is one of the few exceptions with periodic self-healing synchronization.

### Graceful Shutdown

Graceful shutdown behavior is jointly implemented by `apps/gproxy/src/main.rs` and `apps/gproxy/src/workers/mod.rs`:

1. The process listens for `Ctrl+C`; on Unix it also listens for `SIGTERM`.
2. Once shutdown is triggered, the Axum server enters the `with_graceful_shutdown` flow and stops accepting new requests.
3. The main thread then calls `worker_set.shutdown()`, broadcasting the shutdown signal to all workers.
4. `WorkerSet` waits up to 5 seconds for workers to drain.
5. `UsageSink` closes its receiver, drains remaining usage messages, and performs a final batch write.
6. `HealthBroadcaster` flushes any health states still in its debounce window to the database.
7. `QuotaReconciler` and `RateLimitGC` exit their next loop iteration upon receiving the signal.
8. If any workers have not finished within 5 seconds, the process logs a warning but does not block indefinitely.
