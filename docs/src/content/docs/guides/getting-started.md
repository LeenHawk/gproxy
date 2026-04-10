---
title: Quick Start
description: First run to first request in under 5 minutes.
---

## 1. Download the binary

Grab the latest release for your platform from [GitHub Releases](https://github.com/LeenHawk/gproxy/releases).

## 2. Run it

```bash
./gproxy
```

On first run with no config file and no existing database, GPROXY will:

1. Create `./data/gproxy.db` (SQLite).
2. Generate a random admin password and API key.
3. Print both to stdout.

Watch the logs for lines like:

```
INFO generated bootstrap admin password (save this!) admin_user="admin" admin_password="..."
INFO generated bootstrap admin API key (save this!) admin_user="admin" admin_api_key="..."
```

Save these. The password is for console login; the API key is for API access.

## 3. Override defaults with CLI flags

You can override any bootstrap value:

```bash
./gproxy \
  --admin-user myuser \
  --admin-password mysecretpassword \
  --admin-api-key sk-my-admin-key \
  --host 0.0.0.0 \
  --port 9090 \
  --dsn "postgres://user:pass@localhost/gproxy"
```

All flags also accept environment variables: `GPROXY_HOST`, `GPROXY_PORT`, `GPROXY_ADMIN_USER`, `GPROXY_ADMIN_PASSWORD`, `GPROXY_ADMIN_API_KEY`, `GPROXY_DSN`, `GPROXY_DATA_DIR`, `GPROXY_PROXY`, `GPROXY_CONFIG`, `GPROXY_SPOOF`.

## 4. Verify the API

```bash
curl http://127.0.0.1:8787/v1/models \
  -H "Authorization: Bearer <admin-api-key>"
```

If you get a JSON model list back, auth and routing are working.

## 5. Open the console

Point your browser at `http://127.0.0.1:8787/`. It redirects to `/console/login`. Log in with the admin username and password from step 2.

From the console you can add providers, manage users and API keys, view usage, and configure model routing.

## 6. Add your first provider

### Option A: Console UI

In the console, go to Providers and add a new provider. Pick a channel (e.g. `openai`), set the base URL, and add at least one credential with your API key.

### Option B: TOML seed

Create a `gproxy.toml` in the working directory:

```toml
[[providers]]
name = "my-openai"
channel = "openai"

[providers.settings]
base_url = "https://api.openai.com"

[[providers.credentials]]
secret = "sk-your-openai-key-here"
```

Then start (or restart) GPROXY:

```bash
./gproxy
```

On first run, GPROXY reads `gproxy.toml`, seeds everything into the database, and generates the admin credentials. On subsequent runs it loads from the database and ignores the TOML file (the data already exists).

### Option C: Multi-provider TOML

```toml
[[providers]]
name = "openai-main"
channel = "openai"

[providers.settings]
base_url = "https://api.openai.com"

[[providers.credentials]]
secret = "sk-key-1"

[[providers.credentials]]
secret = "sk-key-2"

[[providers]]
name = "claude"
channel = "anthropic"

[providers.settings]
base_url = "https://api.anthropic.com"

[[providers.credentials]]
secret = "sk-ant-your-key"
```

Multiple credentials per provider are load-balanced with health-aware selection. If one key hits a rate limit, GPROXY fails over to the next.

## 7. Send a request

Unscoped (GPROXY routes based on model name):

```bash
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer <api-key>" \
  -H "Content-Type: application/json" \
  -d '{"model": "gpt-4o", "messages": [{"role": "user", "content": "hello"}]}'
```

Provider-scoped (explicit provider target):

```bash
curl http://127.0.0.1:8787/openai-main/v1/chat/completions \
  -H "Authorization: Bearer <api-key>" \
  -H "Content-Type: application/json" \
  -d '{"model": "gpt-4o", "messages": [{"role": "user", "content": "hello"}]}'
```
