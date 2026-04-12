---
title: Installation
description: How to install GPROXY from source, from a release binary, or via Docker.
---

GPROXY ships as a single static binary with the React console embedded inside.
You can get it three ways:

1. **Build from source** (recommended for development).
2. **Download a release binary** from the GitHub releases page.
3. **Run the Docker image** built from [`Dockerfile.action`](https://github.com/LeenHawk/gproxy/blob/main/Dockerfile.action).

## Prerequisites

- **Rust 1.80+** (edition 2024, matching `rust-toolchain` if present).
- **pnpm 9+** and **Node 20+** — only if you intend to rebuild the embedded
  console frontend.
- A supported database — **SQLite** works out of the box, **PostgreSQL** or
  **MySQL** if you prefer a managed backend.

## Build from source

Clone the repository and build in release mode:

```bash
git clone https://github.com/LeenHawk/gproxy.git
cd gproxy
cargo build -p gproxy --release
```

The resulting binary is at `target/release/gproxy`.

### Rebuilding the embedded console

If you touched anything under `frontend/console/`, rebuild the console before
rebuilding the binary so the new assets get baked in:

```bash
cd frontend/console
pnpm install
pnpm build
cd ../..
cargo build -p gproxy --release
```

`pnpm build` writes the compiled assets to the location that the server crate
consumes via `include_dir!` — there is no separate static-file directory to
deploy.

## Release binary

Pre-built binaries for tagged releases are published on GitHub. Download the
archive for your platform, extract the `gproxy` executable, make it
executable, and run it:

```bash
chmod +x gproxy
./gproxy
```

## Docker

A reference Docker image is built via [`Dockerfile.action`](https://github.com/LeenHawk/gproxy/blob/main/Dockerfile.action).
See the [Docker deployment guide](/deployment/docker/) for a complete example,
including mounting a data directory and passing environment variables.

## Next steps

- Follow the [Quick Start](/getting-started/quick-start/) to boot a working
  instance with one provider and one user.
- Skim the [Environment Variables](/reference/environment-variables/)
  reference so you know what knobs are available on first launch.
