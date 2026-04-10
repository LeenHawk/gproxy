---
title: Quick Start
description: Boot GPROXY from scratch and verify your first request.
---

This page provides the minimum startup flow to get a working proxy instance quickly.

## 1. Prepare dependencies

Download the binary for your platform from [release](https://github.com/LeenHawk/gproxy/releases).

## 2. Prepare config file

You can start from [the full example config](https://github.com/LeenHawk/gproxy/blob/main/gproxy.example.full.toml).

Minimal working example:

```toml
[global]
host = "127.0.0.1"
port = 8787
proxy = ""
admin_key = "replace-with-strong-admin-key"
mask_sensitive_info = true
data_dir = "./data"
dsn = "sqlite://./data/gproxy.db?mode=rwc"

[[channels]]
id = "openai"
enabled = true

[channels.settings]
base_url = "https://api.openai.com"

[[channels.credentials]]
secret = "sk-replace-me"
```

## 3. Start service

Run the downloaded binary directly:

```bash
# Linux / macOS
./gproxy
```

```bash
# Windows
gproxy.exe
```

After startup, logs should show:

- Listen address (default `http://127.0.0.1:8787`)
- Current admin key (`password:`)

If `gproxy.toml` is missing, the service starts with in-memory defaults and auto-generates a 16-char admin key.

## 4. Send a minimal verification request

```bash
curl -sS http://127.0.0.1:8787/v1/models \
  -H "x-api-key: <your user key or admin key>"
```

or just provider-scoop like

```bash
curl -sS http://127.0.0.1:8787/claudecode/v1/models \
  -H "x-api-key: <your user key or admin key>"
```

If you get a model list JSON, routing, auth, and upstream connectivity are all working.

## 5. Open admin console

- Console entry: `GET /`
- Static assets path: `/assets/*`
