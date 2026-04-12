---
title: Environment Variables
description: Full list of environment variables consumed by GPROXY at startup.
---

GPROXY's startup arguments are defined in `apps/gproxy/src/main.rs` and
parsed by [`clap`](https://crates.io/crates/clap). Every CLI flag has a
matching environment variable. When both are provided, **CLI arguments
take priority**.

## Runtime

| Variable | Default | Required | Description |
| --- | --- | --- | --- |
| `GPROXY_HOST` | `127.0.0.1` | No | Listen address. |
| `GPROXY_PORT` | `8787` | No | Listen port. |
| `GPROXY_PROXY` | None | No | Upstream HTTP proxy used when calling LLM providers. |
| `GPROXY_SPOOF` | `chrome_136` | No | TLS fingerprint emulation name. |

## Admin bootstrap

Used only during first-run bootstrap to create or reconcile the admin
account if the seed config doesn't define one.

| Variable | Default | Required | Description |
| --- | --- | --- | --- |
| `GPROXY_ADMIN_USER` | `admin` | No | Admin username. |
| `GPROXY_ADMIN_PASSWORD` | None | No | Admin password. If unset and an admin must be created, GPROXY **generates one and logs it once**. |
| `GPROXY_ADMIN_API_KEY` | None | No | Admin API key. If unset and an admin must be created, GPROXY **generates one and logs it once**. |

## Storage

| Variable | Default | Required | Description |
| --- | --- | --- | --- |
| `GPROXY_DSN` | `sqlite://<data_dir>/gproxy.db?mode=rwc` (generated) | No | Database DSN. |
| `GPROXY_DATA_DIR` | `./data` | No | Data directory. The default SQLite file and other runtime state derive from this. |
| `GPROXY_CONFIG` | `gproxy.toml` | No | TOML config path used as the seed file on first-time initialization. |
| `DATABASE_SECRET_KEY` | None | No | At-rest encryption key. When set, passwords / API keys / credentials are encrypted with XChaCha20-Poly1305. |
| `GPROXY_REDIS_URL` | None | No | Redis DSN. Only used when the binary is built with the `redis` feature. |

## Behavior notes

- **CLI precedence.** All of the variables above have matching CLI flags.
  If you set both, the CLI wins.
- **Persisted global settings.** If the database already contains a
  `global_settings` row and you did not pass `GPROXY_DSN` / `GPROXY_DATA_DIR`
  explicitly at startup, GPROXY reconnects using the **persisted**
  configuration — not the environment defaults. This is intentional: it
  lets you change host / port / DSN permanently from the console.
- **Bootstrap logging.** Generated admin passwords and API keys are logged
  **once**, at INFO level, on the first run that creates them. Capture the
  output of that first run, or pass the values in via environment.

## Example

Minimal dev run:

```bash
./gproxy
```

Production-ish run:

```bash
GPROXY_HOST=0.0.0.0 \
GPROXY_PORT=8787 \
GPROXY_DATA_DIR=/var/lib/gproxy \
GPROXY_DSN=postgres://gproxy:secret@db.internal:5432/gproxy \
DATABASE_SECRET_KEY=$(cat /run/secrets/gproxy_db_key) \
GPROXY_CONFIG=/etc/gproxy/seed.toml \
GPROXY_ADMIN_USER=ops \
GPROXY_ADMIN_PASSWORD=$(cat /run/secrets/gproxy_admin_pw) \
./gproxy
```
