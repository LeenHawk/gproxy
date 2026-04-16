---
title: Quick Start
description: Boot a working GPROXY instance with one provider, one user, and one API key in five minutes.
---

This page gets you from zero to a running GPROXY with a real upstream, an
admin account, and a user API key that can make requests.

## 1. Write a seed TOML config

The TOML file pointed to by `GPROXY_CONFIG` is only used the **first time**
GPROXY starts — it seeds the database. After that, the database is the
source of truth and you manage everything from the console or the admin API.

Save this as `gproxy.toml` somewhere on disk. The snippet below creates one
upstream provider, one real model, and an **admin** user with a wildcard
permission — enough to log in and start issuing requests immediately.

```toml
[global]
host = "127.0.0.1"
port = 8787
dsn = "sqlite://./data/gproxy.db?mode=rwc"
data_dir = "./data"

[[providers]]
name = "openai-main"
channel = "openai"
settings = { base_url = "https://api.openai.com/v1" }
credentials = [
  { api_key = "sk-your-upstream-key" }
]

[[models]]
provider_name = "openai-main"
model_id = "gpt-4.1-mini"
display_name = "GPT-4.1 mini"
enabled = true
price_each_call = 0.0

# Admin account — used for the console and the /admin/* API.
[[users]]
name = "admin"
password = "change-me"
is_admin = true
enabled = true

[[users.keys]]
api_key = "sk-admin-1"
label = "default"
enabled = true

# Wildcard permission: admin can call every model on every provider.
[[permissions]]
user_name = "admin"
model_pattern = "*"
```

:::tip
If the seed already defines an `is_admin = true` user with at least one
enabled key (like this one), the `GPROXY_ADMIN_*` bootstrap is **skipped**
entirely. Define non-admin users later from the console.
:::

See the [TOML Config reference](/reference/toml-config/) for every field this
file supports.

## 2. Launch GPROXY

You can point GPROXY at the seed TOML in three equivalent ways — pick one:

```bash
# (a) environment variable
GPROXY_CONFIG=./gproxy.toml ./gproxy

# (b) command-line flag
./gproxy --config ./gproxy.toml

# (c) default discovery — no flag needed if the file is named
# `gproxy.toml` and sits in the current working directory
./gproxy
```

Every setting has both an env var and a `--flag`; the flag wins. Common
ones:

| Flag | Env | Default |
|------|-----|---------|
| `--config` | `GPROXY_CONFIG` | `gproxy.toml` |
| `--host` | `GPROXY_HOST` | `127.0.0.1` |
| `--port` | `GPROXY_PORT` | `8787` |
| `--dsn` | `GPROXY_DSN` | sqlite under `--data-dir` |
| `--data-dir` | `GPROXY_DATA_DIR` | `./data` |

Run `./gproxy --help` for the full list. See the
[Environment Variables reference](/reference/environment-variables/)
for the complete table.

On the first run GPROXY will:

1. Create `./data/gproxy.db` (SQLite) automatically.
2. Import the seed TOML into the database.
3. Start the HTTP server on `127.0.0.1:8787`.

Because the seed already defines an admin account, **you do not need**
`GPROXY_ADMIN_USER` / `GPROXY_ADMIN_PASSWORD` / `GPROXY_ADMIN_API_KEY`.
They are only used when the seed has no admin.

:::tip
If you're running without a seed TOML, set those three environment
variables (or pass `--admin-user` / `--admin-password` /
`--admin-api-key`) so GPROXY can bootstrap an admin on first launch.
When they are unset, GPROXY generates a password and API key and
**logs them once** — copy them immediately.
:::

## 3. Open the console

Navigate to **<http://127.0.0.1:8787/console>**, log in as `admin` /
`change-me`, and you should see:

- The `openai-main` provider you seeded.
- `gpt-4.1-mini` listed under its models.
- The `admin` user with key `sk-admin-1` and a wildcard permission.

From here, use the console's *Users* tab to create additional (non-admin)
users and scope their model access — see
[Users & API Keys](/guides/users-and-keys/) and
[Permissions, Rate Limits & Quotas](/guides/permissions/).

## 4. Make your first request

You're now ready to send a real request. The admin key works against the
LLM routes just like any other user key.

See [First Request](/getting-started/first-request/) for the full example,
including how to use model aliases and how the Claude / Gemini compatible
endpoints work.

## 5. Forced-thinking / effort variants via aliases

GPROXY does not expose "force-thinking" as a server flag — instead, any
model variant you want clients to call (e.g. a `gpt-5-thinking-high` that
always forces high reasoning effort, or a `claude-thinking-low` that
always enables 1024 thinking tokens) is created as an **alias with a
suffix preset** in the console's *Models* tab.

Each suffix preset attaches a small set of body-rewrite rules (for
example, forcing `"thinking": { "type": "enabled", "budget_tokens":
32768 }` on every Claude request, or pinning `"reasoning_effort":
"high"` on every Chat Completions request). Clients call the alias name
and the rewrite fires automatically before the request is dispatched
upstream.

See [Models & Aliases](/guides/models/) for the full alias pipeline,
and [Rewrite Rules](/guides/rewrite-rules/) for what the suffix presets
emit under the hood.
