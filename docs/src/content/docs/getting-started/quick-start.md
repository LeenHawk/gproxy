---
title: Quick Start
description: Start gproxy v2 locally, seed a small config bundle, open Console, and make the first request.
---

This guide starts a local native gproxy v2 instance with an embedded console and
a minimal import bundle. It uses the current v2 config model: runtime settings
come from CLI flags or environment variables, while provider, route, user, key,
and rule records live in persistence and can be imported as JSON.

## 1. Build Or Install

Use a release binary, Docker image, or local source build. For a source build
with the console embedded:

```bash
cd console
pnpm install --frozen-lockfile
pnpm build
cd ..

cargo build --release --bin gproxy
```

The binary is `target/release/gproxy`.

## 2. Prepare A Dev Import Bundle

The docs site includes a development bundle at
`docs/public/examples/import-dev.json`. Copy it outside the docs tree before
putting real upstream keys in it:

```bash
cp docs/public/examples/import-dev.json ./import-dev.local.json
```

Edit the copied file and replace:

- `sk-REPLACE` with an OpenAI-compatible upstream key.
- `sk-ant-REPLACE` with an Anthropic-compatible upstream key if you want to test
  the Claude provider too.

The example bundle creates:

- org `default`;
- admin user `dev`;
- user API key `sk-dev-local`;
- provider `openai-main`;
- route `main`, pointing to upstream model `gpt-4.1-mini`;
- a wildcard route permission for the default org.

:::caution
`import-dev.local.json` contains plaintext upstream credentials and user API
keys. Keep it local and do not commit it.
:::

## 3. Start gproxy

Start the native binary with a local data directory and ask the first-boot hook
to import the bundle if the store is empty:

```bash
GPROXY_DATA_DIR=./data \
GPROXY_IMPORT_FILE=./import-dev.local.json \
GPROXY_ADMIN_USER=dev \
GPROXY_ADMIN_PASSWORD=change-me-please \
./target/release/gproxy
```

Useful defaults:

| Setting | Default |
| --- | --- |
| `GPROXY_HOST` | `127.0.0.1` |
| `GPROXY_PORT` | `8787` |
| `GPROXY_PERSISTENCE` | `db` |
| `GPROXY_DATA_DIR` | `./data` |
| `GPROXY_DSN` | `sqlite://<data_dir>/gproxy.db?mode=rwc` when unset |

`GPROXY_IMPORT_FILE` imports only when providers and users are both empty. Once
the store has data, the same env var is ignored.

`GPROXY_ADMIN_USER=dev` makes the recovery override target the admin user from
the import bundle. `GPROXY_ADMIN_PASSWORD` force-upserts that named admin user on
every startup while it is set. Remove it after the first login if you do not
want a host-level password reset path.

For encrypted at-rest secrets, set `GPROXY_MASTER_KEY` to standard base64 for
exactly 32 bytes. Without it, v2 runs in plaintext secret mode and logs a
warning.

## 4. Open Console

Open <http://127.0.0.1:8787/console>.

The development bundle's user is `dev`, and the command above forces its admin
password to `change-me-please`. From Console you can review providers,
credentials, routes, route members, route permissions, rate limits, quotas,
usage, logs, and update settings.

## 5. Make A Gateway Request

Use the imported user key:

```bash
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer sk-dev-local" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "main",
    "messages": [
      { "role": "user", "content": "Say hello in one short sentence." }
    ]
  }'
```

The aggregated `/v1` endpoint resolves `main` as a v2 route. The selected route
member rewrites the upstream model to `gpt-4.1-mini` before dispatch.

For provider-scoped requests, use `/{provider}/v1/...`:

```bash
curl http://127.0.0.1:8787/openai-main/v1/chat/completions \
  -H "Authorization: Bearer sk-dev-local" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4.1-mini",
    "messages": [
      { "role": "user", "content": "Say hello in one short sentence." }
    ]
  }'
```

Continue with [First Request](/getting-started/first-request/) for the routing
rules behind those two forms.
