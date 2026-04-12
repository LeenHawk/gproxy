---
title: TOML Config
description: The seed TOML file read by GPROXY_CONFIG on first-time initialization.
---

The file pointed to by `GPROXY_CONFIG` (default: `gproxy.toml`) is a
**seed config**. It is only consulted on the **first** startup where the
target database does not yet contain data. From then on, the database is
the source of truth — re-running with a modified TOML will not reimport
it.

The exact structure is defined in
[`crates/gproxy-api/src/admin/config_toml.rs`](https://github.com/LeenHawk/gproxy/blob/main/crates/gproxy-api/src/admin/config_toml.rs).
The sections below describe every supported table.

## Example

A minimal but realistic seed: one provider, one model, and an **admin**
user with wildcard access to every model on every provider. Everything
else (more providers, non-admin users, aliases, rate limits, quotas) can
be added later from the console or by extending this file before the
first run.

```toml
[global]
host = "0.0.0.0"
port = 8787
proxy = "http://127.0.0.1:7890"
spoof_emulation = "chrome_136"
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

# Admin user — logs into the console and calls /admin/*
[[users]]
name = "admin"
password = "change-me"              # plaintext or Argon2 PHC
is_admin = true
enabled = true

[[users.keys]]
api_key = "sk-admin-1"
label = "default"
enabled = true

# Wildcard permission for the admin.
# `provider_name` is omitted so it matches every provider.
[[permissions]]
user_name = "admin"
model_pattern = "*"
```

### Adding a non-admin user

If you want to seed a regular user alongside the admin, append something
like this to the same file:

```toml
[[users]]
name = "alice"
password = "plain-text-or-argon2-phc"
enabled = true

[[users.keys]]
api_key = "sk-user-alice-1"
label = "default"
enabled = true

# Scoped permission: alice can only call gpt-* on openai-main.
[[permissions]]
user_name = "alice"
provider_name = "openai-main"
model_pattern = "gpt-*"

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

# Optional: aliases, file permissions, etc.
[[model_aliases]]
alias = "chat-default"
provider_name = "openai-main"
model_id = "gpt-4.1-mini"
enabled = true

[[file_permissions]]
user_name = "alice"
provider_name = "openai-main"
```

## `[global]`

Top-level runtime settings. Every field is optional; anything you omit
falls back to the matching environment variable or built-in default.

| Field | Description |
| --- | --- |
| `host`, `port` | Listen address and port. |
| `proxy` | Upstream HTTP proxy used when calling LLM providers. |
| `spoof_emulation` | TLS fingerprint emulation name. |
| `enable_usage` | Turns on usage accounting. |
| `enable_upstream_log` / `enable_upstream_log_body` | Capture upstream envelope / body. |
| `enable_downstream_log` / `enable_downstream_log_body` | Capture downstream envelope / body. |
| `dsn` | Database DSN. |
| `data_dir` | Data directory. |

## `[[providers]]`

One entry per upstream provider.

| Field | Description |
| --- | --- |
| `name` | Unique provider name. |
| `channel` | Channel type (see [Providers & Channels](/guides/providers/)). |
| `settings` | JSON value passed to the channel's settings type. |
| `credentials` | Array of JSON values, each a credential understood by the channel. |

`settings` and each `credentials[i]` are deserialized via `serde_json::Value`,
so the exact schema depends on the channel (`OpenAiSettings`,
`AnthropicCredential`, and so on).

## `[[models]]` and `[[model_aliases]]`

`[[models]]` defines a forwardable real model on a provider, optionally
with pricing. `[[model_aliases]]` defines an alias pointing at a
`(provider, model_id)` pair. On import, both end up in the unified
`models` table (aliases get a non-null `alias_of` column).

## `[[users]]` and `[[users.keys]]`

`[[users]]` defines an account. `password` may be plain text (GPROXY hashes
with Argon2 on import) or a direct Argon2 PHC string. Set `is_admin = true`
to create an admin — that account can log into the console and call
`/admin/*`.

`[[users.keys]]` is a nested array-of-tables listing the API keys for the
preceding user. Each key has `api_key`, `label`, and `enabled`.

:::tip
An admin user still needs at least one `[[permissions]]` row to call LLM
routes — a common pattern is a single wildcard entry
(`model_pattern = "*"`, `provider_name` omitted). Being `is_admin` gates
administrative endpoints, not model access.
:::

## Access control tables

| Table | Purpose |
| --- | --- |
| `[[permissions]]` | `(user, provider, model_pattern)` grants model access. |
| `[[file_permissions]]` | Grants file-endpoint access per provider. |
| `[[rate_limits]]` | Per-user, per-pattern `rpm` / `rpd` / `total_tokens` ceilings. |
| `[[quotas]]` | USD ceilings with `quota` and `cost_used` per user. |

See [Permissions, Rate Limits & Quotas](/guides/permissions/) for the
semantics and the order in which these are evaluated.

## Bootstrap behavior

On startup, GPROXY checks whether the database already has data:

- **Empty database:** the TOML seed is imported. The admin account is
  either picked up from the seed (a user with `is_admin = true` and an
  enabled key) or bootstrapped from `GPROXY_ADMIN_USER`,
  `GPROXY_ADMIN_PASSWORD`, and `GPROXY_ADMIN_API_KEY`.
- **Non-empty database:** the TOML is ignored entirely. Edit live state
  from the console or the admin API instead.
