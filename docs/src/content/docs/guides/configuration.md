---
title: Configuration
description: Multi-database, native channels, custom channels, and dispatch conversion settings.
---

## Config entry points

Start from these files:

- `gproxy.example.toml`: minimum runnable example
- `gproxy.example.full.toml`: full field reference

## Override priority

Runtime priority:

`CLI args / env vars > gproxy.toml > defaults`

Note:

- Once DB is already initialized, bootstrap prefers DB state by default (unless forced by startup switch below).

Common overrides:

- `--config` / `GPROXY_CONFIG_PATH`
- `--host` / `GPROXY_HOST`
- `--port` / `GPROXY_PORT`
- `--proxy` / `GPROXY_PROXY`
- `--admin-key` / `GPROXY_ADMIN_KEY`
- `--bootstrap-force-config` / `GPROXY_BOOTSTRAP_FORCE_CONFIG`
- `--mask-sensitive-info` / `GPROXY_MASK_SENSITIVE_INFO`
- `--data-dir` / `GPROXY_DATA_DIR`
- `--dsn` / `GPROXY_DSN`
- `--database-secret-key` / `DATABASE_SECRET_KEY`

## Bootstrap source mode

Startup-only switch (CLI/env only, not a `gproxy.toml` field):

- `--bootstrap-force-config` / `GPROXY_BOOTSTRAP_FORCE_CONFIG`

Behavior:

- default (`false` or unset):
  - if DB is not initialized, bootstrap from `gproxy.toml`.
  - if DB is initialized, prefer DB state and skip config-file channel/provider import.
  - startup-provided `admin_key` override is still honored.
- `true`:
  - force applying config-file channels/settings/credentials/global values on boot.
  - useful when intentionally overwriting existing DB bootstrap state from file.

## Multi-database support (key)

`gproxy-storage` enables `sqlite + mysql + postgres`. Switch backend by changing `global.dsn`.

Examples:

```toml
# SQLite (default)
dsn = "sqlite://./data/gproxy.db?mode=rwc"
```

```toml
# MySQL
dsn = "mysql://user:password@127.0.0.1:3306/gproxy"
```

```toml
# PostgreSQL
dsn = "postgres://user:password@127.0.0.1:5432/gproxy"
```

## Database at-rest encryption

Configure the database encryption key via CLI or env var:

```bash
./gproxy --database-secret-key 'replace-with-long-random-string'
```

```bash
export DATABASE_SECRET_KEY='replace-with-long-random-string'
./gproxy
```

Behavior:

- `DATABASE_SECRET_KEY` unset: sensitive fields are stored and read as plaintext;
- `DATABASE_SECRET_KEY` set: `credential.secret_json`, user API keys, user passwords, `admin_key`, and `hf_token` are transparently encrypted at rest.

Recommendations:

- set the key before the first database bootstrap and keep it identical on every instance using the same database;
- on free-tier or shared managed databases, strongly prefer setting the key so sensitive values are not stored in plaintext;
- inject it through env vars / platform secrets instead of committing it to the repo or checked-in config;
- once encrypted data exists, do not change the key casually; rotate it only after a migration / re-encryption plan.

## `global`

| Field | Description |
|---|---|
| `host` | Listen address, default `127.0.0.1` |
| `port` | Listen port, default `8787` |
| `proxy` | Upstream proxy; empty string means disabled |
| `hf_token` | Optional HuggingFace token |
| `hf_url` | HuggingFace base URL, default `https://huggingface.co` |
| `admin_key` | Admin key; can be auto-generated on first run |
| `mask_sensitive_info` | Mask sensitive fields in logs/events |
| `data_dir` | Data directory, default `./data` |
| `dsn` | DB DSN (sqlite/mysql/postgres) |

## `runtime`

| Field | Default | Description |
|---|---:|---|
| `storage_write_queue_capacity` | `4096` | Storage write queue capacity |
| `storage_write_max_batch_size` | `1024` | Max events per write batch |
| `storage_write_aggregate_window_ms` | `25` | Aggregation window in ms |

## `channels` (native and custom)

Define each channel with `[[channels]]`:

- `id`: channel ID (built-in like `openai`, or custom like `mycustom`)
- `enabled`: whether enabled
- `settings`: channel settings (usually includes `base_url`)
- `dispatch`: optional protocol dispatch rules
- `credentials`: credential list (supports multiple credentials)

Example:

```toml
[[channels]]
id = "openai"
enabled = true

[channels.settings]
base_url = "https://api.openai.com"

[[channels.credentials]]
id = "openai-main"
label = "primary"
secret = "sk-replace-me"
```

## Built-in channel capability matrix (key)

| Channel | `id` | OAuth | `/v1/usage` | `secret` credential |
|---|---|---|---|---|
| OpenAI | `openai` | No | No | Yes |
| Anthropic | `anthropic` | No | No | Yes |
| AiStudio | `aistudio` | No | No | Yes |
| VertexExpress | `vertexexpress` | No | No | Yes |
| Vertex | `vertex` | No | No | No (service account) |
| GeminiCli | `geminicli` | Yes | Yes | No (OAuth builtin) |
| ClaudeCode | `claudecode` | Yes | Yes | No (OAuth/Cookie builtin) |
| Codex | `codex` | Yes | Yes | No (OAuth builtin) |
| Antigravity | `antigravity` | Yes | Yes | No (OAuth builtin) |
| Nvidia | `nvidia` | No | No | Yes |
| Deepseek | `deepseek` | No | No | Yes |
| Groq | `groq` | No | No | Yes |

## Claude / ClaudeCode cache rewrite (`cache_breakpoints`)

`anthropic` and `claudecode` use `channels.settings.cache_breakpoints` for cache-control rewrite.

Rule model:

- key: `channels.settings.cache_breakpoints`
- value: array, max `4` rules
- supported `target`:
  - `top_level` (alias: `global`)
  - `tools`
  - `system`
  - `messages`
- for non-`top_level` targets:
  - `position`: `nth` or `last_nth`
  - `index`: 1-based
  - for `messages`, the index is resolved against flattened `messages[*].content` blocks after shorthand normalization (`content: "..."` -> one text block)
  - for `messages`, if `content_position` / `content_index` is set, `position` / `index` first selects a message and `content_*` then selects a block inside that message
- for `top_level` target, `position` / `index` are ignored
- `ttl`: `auto` | `5m` | `1h`
  - `auto` injects `{"type":"ephemeral"}` without `ttl`

Rewrite behavior:

- existing request-side `cache_control` is preserved and counts toward the 4-breakpoint budget
- gproxy only fills remaining slots and never overwrites existing block/top-level `cache_control`
- magic-string-triggered insertion shares the same 4-breakpoint budget
- only `anthropic` / `claudecode` message-generation requests are rewritten
- Admin UI sorts rules before submit (`top_level -> tools -> system -> messages`), then server keeps the first 4

Default TTL note when `ttl` is omitted (`auto`):

- `anthropic`: upstream default is `5m`
- `claudecode`: upstream default is `5m`
- if you need deterministic behavior, set `ttl` explicitly to `5m` or `1h`

Example:

```toml
[[channels]]
id = "anthropic"
enabled = true

[channels.settings]
base_url = "https://api.anthropic.com"
cache_breakpoints = [
  { target = "top_level", ttl = "auto" },
  { target = "system", position = "last_nth", index = 1, ttl = "auto" },
  { target = "messages", position = "last_nth", index = 11, ttl = "auto" },
  { target = "messages", position = "last_nth", index = 1, content_position = "last_nth", content_index = 1, ttl = "5m" }
]

[[channels]]
id = "claudecode"
enabled = true

[channels.settings]
base_url = "https://api.anthropic.com"
cache_breakpoints = [
  { target = "top_level", ttl = "auto" },
  { target = "messages", position = "last_nth", index = 1, content_position = "last_nth", content_index = 1, ttl = "1h" }
]
```

## Custom channel example (key)

```toml
[[channels]]
id = "mycustom"
enabled = true

[channels.settings]
base_url = "https://api.example.com"

[[channels.credentials]]
secret = "custom-provider-api-key"
```

Notes:

- Custom channels use `ProviderDispatchTable::default_for_custom()` by default.
- You can explicitly provide `dispatch` for fine-grained protocol routing.

## `channels.credentials`

Available fields:

- `id` / `label`: human-readable identifiers
- `secret`: API key credential
- `builtin`: structured OAuth / ServiceAccount credential
- `state`: health status seed

Health status types:

- `healthy`: available
- `partial`: model-level cooldown (partially available)
- `dead`: unavailable

## Credential selection mode

In `channels.settings`, you can control multi-credential routing with:

- `credential_round_robin_enabled`
- `credential_cache_affinity_enabled`
- `credential_cache_affinity_max_keys`

For full behavior matrix, internal affinity-pool design, hit judgment, and upstream OpenAI/Claude/Gemini cache strategy guidance, see:

- [Credential Selection and Cache Affinity](/guides/credential-selection-cache-affinity/)

## Dispatch and conversion

`dispatch` defines how a request is executed:

- `Passthrough`: forward as-is
- `TransformTo`: transform to target protocol then forward
- `Local`: local implementation
- `Unsupported`: explicitly unsupported

This is the core mechanism that enables multiple protocol entrances across heterogeneous upstream providers.
