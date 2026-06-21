---
title: Release Build
description: Build a production gproxy v2 native binary with the embedded console.
---

The native release build is a single-crate Rust build plus an optional console
asset build. The release workflow runs both: it builds `console/`, uploads the
synced `assets/console/` directory, and then builds `--bin gproxy` for each
native target.

## Build The Console

Run this when console source, translations, routing, or styling changed:

```bash
cd console
pnpm install --frozen-lockfile
pnpm build
cd ..
```

`pnpm build` performs:

1. `tsc -b`
2. `vite build`
3. `node ./scripts/sync-to-embed.mjs`

The final step copies `console/dist/` into `assets/console/`. The native server
embeds that directory through `rust-embed` and serves it at `/console`.

## Build The Native Binary

From the repository root:

```bash
cargo build --release --bin gproxy
```

The output is:

```text
target/release/gproxy
```

For a target-specific build:

```bash
cargo build --release --bin gproxy --target x86_64-unknown-linux-gnu
```

The release workflow builds Linux glibc, Linux musl, macOS, Windows, and Android
targets. It also smoke-checks selected binaries with `--help` before packaging.

## Runtime Configuration

The binary is configured by CLI flags and environment variables. There is no v2
TOML runtime config file.

Common settings:

| CLI | Env | Default |
| --- | --- | --- |
| `--host` | `GPROXY_HOST` | `127.0.0.1` |
| `--port` | `GPROXY_PORT` | `8787` |
| `--persistence` | `GPROXY_PERSISTENCE` | `db` |
| `--data-dir` | `GPROXY_DATA_DIR` | `./data` |
| `--dsn` | `GPROXY_DSN` | SQLite under `<data-dir>/gproxy.db` |
| `--redis-url` | `GPROXY_REDIS_URL` | in-process memory cache |
| `--admin-user` | `GPROXY_ADMIN_USER` | `admin` |
| `--admin-password` | `GPROXY_ADMIN_PASSWORD` | random first-boot password if needed |

`GPROXY_MASTER_KEY` is env-only. It must be standard base64 for exactly 32 bytes
when you want v2 to seal credentials and user API keys at rest.

## Package A Binary

For a simple archive:

```bash
mkdir -p dist
cp target/release/gproxy dist/gproxy
cp README.md dist/
(cd dist && zip -9 ../gproxy-local.zip gproxy README.md)
shasum -a 256 gproxy-local.zip > gproxy-local.zip.sha256
```

The release workflow may UPX-compress selected Linux and Windows artifacts
before packaging. It signs macOS artifacts ad hoc with `codesign --sign -`.

## First Run

On startup the native server:

1. creates `GPROXY_DATA_DIR`;
2. builds the secret cipher from `GPROXY_MASTER_KEY`;
3. auto-migrates a v1 SQLite database when the default v1-to-v2 conditions match;
4. opens the configured persistence backend;
5. imports `GPROXY_IMPORT_FILE` only if providers and users are empty;
6. ensures or recovers the admin user;
7. starts the cache, upstream transport, snapshot, router, console, and gateway.

Use `./gproxy --help` to inspect every current flag on the built binary.
