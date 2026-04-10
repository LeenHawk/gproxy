---
title: Configuration Reference
description: Complete reference for CLI arguments, environment variables, TOML config, database encryption, and multi-database support.
---

## Config Priority

GPROXY resolves configuration from three sources, in this order:

1. **CLI arguments / environment variables** -- always win.
2. **TOML config file** (`gproxy.toml`) -- applied on first bootstrap only.
3. **Database** -- all runtime changes via admin API or console are persisted here.

Once the database is initialized, the TOML file is ignored on subsequent starts. All runtime config changes go through the admin API or console and are persisted to the database.

The startup-only flag `--bootstrap-force-config` / `GPROXY_BOOTSTRAP_FORCE_CONFIG` overrides this behavior:

- `false` (default): if the database already has data, prefer DB state and skip TOML import.
- `true`: force-apply TOML channels, settings, credentials, and global values on boot. Use this when intentionally overwriting existing DB state from file.

## CLI Arguments and Environment Variables

| Argument | Env Var | Default | Description |
|----------|---------|---------|-------------|
| `--host` | `GPROXY_HOST` | `127.0.0.1` | Listen address |
| `--port` | `GPROXY_PORT` | `8787` | Listen port |
| `--admin-user` | `GPROXY_ADMIN_USER` | `admin` | Bootstrap admin username |
| `--admin-password` | `GPROXY_ADMIN_PASSWORD` | (auto-generated) | Bootstrap admin password |
| `--admin-api-key` | `GPROXY_ADMIN_API_KEY` | (auto-generated) | Bootstrap admin API key |
| `--dsn` | `GPROXY_DSN` | (none; defaults to SQLite) | Database connection string |
| `--config` | `GPROXY_CONFIG` | `gproxy.toml` | TOML config file path |
| `--data-dir` | `GPROXY_DATA_DIR` | `./data` | Data directory |
| `--proxy` | `GPROXY_PROXY` | (none) | Upstream HTTP proxy |
| `--spoof-emulation` | `GPROXY_SPOOF` | `chrome_136` | TLS fingerprint emulation |
| `--database-secret-key` | `DATABASE_SECRET_KEY` | (none) | XChaCha20Poly1305 encryption key for at-rest DB encryption |

When `--admin-password` or `--admin-api-key` are omitted, GPROXY auto-generates random values and logs them at startup. Save them -- they are not logged again.

If the admin user, password, or API key are explicitly provided via CLI/env (even when the DB already has data), the bootstrap admin is reconciled on every start.

## TOML Config Reference

The TOML config file seeds the database on first boot. Fields map directly to database tables. After initial seed, manage everything through the admin API or console.

### `[global]`

Server-wide settings.

| Field | Default | Description |
|-------|---------|-------------|
| `host` | `127.0.0.1` | Listen address |
| `port` | `8787` | Listen port |
| `proxy` | (none) | Upstream HTTP proxy. Empty string disables |
| `spoof_emulation` | `chrome_136` | TLS fingerprint emulation target |
| `update_source` | `github` | Self-update source (`github` or `cloudflare`) |
| `enable_usage` | `true` | Track token usage and costs |
| `enable_upstream_log` | `false` | Log upstream request/response metadata |
| `enable_upstream_log_body` | `false` | Log upstream request/response bodies |
| `enable_downstream_log` | `false` | Log downstream (client) request/response metadata |
| `enable_downstream_log_body` | `false` | Log downstream request/response bodies |
| `dsn` | (derived from data_dir) | Database connection string |
| `data_dir` | `./data` | Data directory for SQLite and other files |

```toml
[global]
host = "0.0.0.0"
port = 8787
proxy = ""
spoof_emulation = "chrome_136"
update_source = "github"
enable_usage = true
enable_upstream_log = false
enable_upstream_log_body = false
enable_downstream_log = false
enable_downstream_log_body = false
dsn = "sqlite://./data/gproxy.db?mode=rwc"
data_dir = "./data"
```

### `[[providers]]`

Each `[[providers]]` block defines an upstream provider instance.

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `name` | yes | -- | Unique provider name (e.g. `openai-prod`) |
| `channel` | yes | -- | Channel type. Built-in: `openai`, `anthropic`, `aistudio`, `vertexexpress`, `vertex`, `geminicli`, `claudecode`, `codex`, `antigravity`, `nvidia`, `deepseek`, `groq`, `custom` |
| `settings` | no | `{}` | JSON object. Must contain at least `base_url` for most channels |
| `credentials` | no | `[]` | Array of credential JSON objects |

```toml
[[providers]]
name = "openai-prod"
channel = "openai"

[providers.settings]
base_url = "https://api.openai.com"

[[providers.credentials]]
api_key = "sk-replace-me"
```

For channels with OAuth credentials (e.g. `claudecode`, `geminicli`, `codex`, `antigravity`), credentials use structured builtin fields instead of a plain API key. See the full example config for details.

### `[[models]]`

Override or add model pricing and display metadata.

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `provider_name` | yes | -- | Must match a provider's `name` |
| `model_id` | yes | -- | Model identifier (e.g. `gpt-4o`) |
| `display_name` | no | (none) | Human-readable label |
| `enabled` | no | `true` | Whether this model is available |
| `price_each_call` | no | (none) | Fixed cost per request |
| `price_tiers` | no | `[]` | Tiered pricing by input token count |

```toml
[[models]]
provider_name = "openai-prod"
model_id = "gpt-4o"
display_name = "GPT-4o"
enabled = true
price_each_call = 0.0
price_tiers = [
  { input_tokens_up_to = 128000, price_input_tokens = 2.5, price_output_tokens = 10.0 }
]
```

Built-in channels ship default model pricing tables. Explicit `[[models]]` entries override them.

### `[[model_aliases]]`

Map an alias model name to a real provider + model.

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `alias` | yes | -- | The alias name clients use |
| `provider_name` | yes | -- | Target provider name |
| `model_id` | yes | -- | Target model ID |
| `enabled` | no | `true` | Whether this alias is active |

```toml
[[model_aliases]]
alias = "gpt-4"
provider_name = "openai-prod"
model_id = "gpt-4o"
enabled = true
```

### `[[users]]`

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `name` | yes | -- | Username |
| `password` | no | `""` | Plaintext password or Argon2 PHC hash |
| `enabled` | no | `true` | Whether this user can authenticate |
| `is_admin` | no | `false` | Admin role flag |

Each user can have nested API keys:

#### `[[users.keys]]`

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `api_key` | yes | -- | The API key string |
| `label` | no | (none) | Human-readable label |
| `enabled` | no | `true` | Whether this key is active |

```toml
[[users]]
name = "alice"
password = "plaintext-or-argon2-hash"
enabled = true
is_admin = false

[[users.keys]]
api_key = "sk-alice-key-1"
label = "dev"
enabled = true
```

Passwords can be supplied as plaintext (hashed automatically on import) or as an Argon2id PHC-format hash string (stored as-is).

### `[[permissions]]`

Grant a user access to models by pattern.

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `user_name` | yes | -- | Must match a user's `name` |
| `provider_name` | no | (none) | Scope to a specific provider. `None` means all providers |
| `model_pattern` | yes | -- | Glob pattern. `*` matches all models |

```toml
[[permissions]]
user_name = "alice"
model_pattern = "*"

[[permissions]]
user_name = "bob"
provider_name = "openai-prod"
model_pattern = "gpt-*"
```

A user with zero permissions cannot call any models.

### `[[file_permissions]]`

Grant a user file upload capability for a specific provider.

| Field | Required | Description |
|-------|----------|-------------|
| `user_name` | yes | Must match a user's `name` |
| `provider_name` | yes | Must match a provider's `name` |

```toml
[[file_permissions]]
user_name = "alice"
provider_name = "anthropic-prod"
```

### `[[rate_limits]]`

Per-user rate limiting by model pattern.

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `user_name` | yes | -- | Must match a user's `name` |
| `model_pattern` | yes | -- | Glob pattern for affected models |
| `rpm` | no | (none) | Requests per minute |
| `rpd` | no | (none) | Requests per day |
| `total_tokens` | no | (none) | Total token budget (lifetime) |

```toml
[[rate_limits]]
user_name = "alice"
model_pattern = "*"
rpm = 60
rpd = 1000
```

Omitting a field means no limit for that dimension.

### `[[quotas]]`

Per-user cost quota.

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `user_name` | yes | -- | Must match a user's `name` |
| `quota` | yes | -- | Maximum allowed cost (USD) |
| `cost_used` | no | `0.0` | Current consumed cost |

```toml
[[quotas]]
user_name = "alice"
quota = 50.0
cost_used = 0.0
```

When `cost_used >= quota`, requests are rejected.

## Database Encryption

Set `--database-secret-key` or `DATABASE_SECRET_KEY` to enable at-rest encryption of sensitive database fields.

### What gets encrypted

- Credential secrets (API keys, OAuth tokens, service account keys)
- User API keys
- User password hashes
- Admin API key

### Algorithm

XChaCha20Poly1305 with an Argon2id-derived 256-bit key. The secret you provide is not used directly -- it is run through Argon2id (19 MiB memory, 2 iterations, 1 lane) with a fixed domain-separator salt to derive the actual encryption key.

Encrypted strings are stored with the prefix `enc:v2:`. Encrypted JSON values are stored as an object with `$gproxy_enc`, `nonce`, and `ciphertext` fields. Unencrypted values are transparently passed through on read.

### Operational rules

- Set the key **before first bootstrap**. All sensitive values written to the database will be encrypted.
- Keep the key **identical across all instances** sharing the same database.
- Inject via environment variable or platform secrets. Do not commit it to source control or config files.
- Do not change the key after data is written. Changing it requires a migration / re-encryption plan.
- When no key is set, sensitive fields are stored as plaintext.

```bash
# Set via environment variable
export DATABASE_SECRET_KEY='your-long-random-secret-string'
./gproxy

# Or via CLI argument
./gproxy --database-secret-key 'your-long-random-secret-string'
```

## Multi-Database Support

GPROXY supports SQLite, MySQL, and PostgreSQL. The database backend is selected by the DSN format.

### SQLite (default)

When no DSN is specified, GPROXY creates a SQLite database at `{data_dir}/gproxy.db`.

```
sqlite://./data/gproxy.db?mode=rwc
```

### MySQL

```
mysql://user:password@127.0.0.1:3306/gproxy
```

### PostgreSQL

```
postgres://user:password@127.0.0.1:5432/gproxy
```

Set the DSN via `--dsn`, `GPROXY_DSN`, or `[global] dsn` in the TOML config.

Database schema is automatically synced on startup. No manual migration is needed.
