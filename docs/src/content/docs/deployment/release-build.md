---
title: Release Build
description: How to produce a production binary of gproxy, including the embedded console.
---

A production build of gproxy is a single Cargo release build plus a
one-time frontend build. Both steps are idempotent and can be wired into
CI.

## 1. Build the embedded console (if changed)

If you modified anything under `frontend/console/`, rebuild the console
first so the new assets get baked into the binary:

```bash
cd frontend/console
pnpm install
pnpm build
cd ../..
```

`pnpm build` writes the compiled assets to the path the server crate
embeds via `include_dir!`. There is no separate static-file directory to
ship with the binary.

If you haven't touched the frontend, you can skip this step — the last
built assets stay on disk and are picked up by the next Cargo build.

## 2. Build the Rust binary

```bash
cargo build -p gproxy --release
```

The output binary is at `target/release/gproxy`. It is statically linked
against the Rust standard library but depends on the system's OpenSSL /
TLS stack unless you build with `rustls` features enabled in your
environment.

## 3. Strip and package (optional)

To reduce the binary size for distribution:

```bash
strip target/release/gproxy
```

You can then ship the stripped binary as-is; it has no runtime
dependencies beyond `libc` and the TLS stack.

## 4. First run

On the first run, gproxy will:

- Create `GPROXY_DATA_DIR` if it doesn't exist.
- Generate a SQLite file under the data directory if `GPROXY_DSN` is unset.
- Import the seed TOML (`GPROXY_CONFIG`) if the database is empty.
- Bootstrap an admin account from `GPROXY_ADMIN_*` if needed, logging
  generated credentials once.
- Start the HTTP server and the worker set.

See the [Quick Start](/getting-started/quick-start/) for a concrete
first-run walkthrough.

## CI tips

- Cache the `~/.cargo` and `target/` directories between CI runs; the
  workspace has many dependencies and re-downloading them dominates
  cold-build time.
- Cache `frontend/console/node_modules` via pnpm's store path for the
  same reason.
- Run `cargo test -p gproxy` and `pnpm test` (if configured) on pull
  requests; save the `release` build for tags.
- If you tag a release, generate release notes from `RELEASE_NOTE.md`
  and attach the stripped binary as an artifact.
