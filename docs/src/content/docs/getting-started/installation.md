---
title: Installation
description: Install GPROXY v2 from a release binary, Docker image, source build, or edge bundle.
---

GPROXY v2 is a single Rust crate that builds one native binary named `gproxy`.
The same crate also builds the edge WebAssembly runtime. The React console is
not a separate service in the native build: after `console/` is built, its
static files are synced into `assets/console/` and embedded in the binary.

Choose the installation path that matches how you want to run it.

## Release Binary

Use a release binary when you want the native server with the embedded console
and no local Rust or Node toolchain.

1. Download the archive for your OS and CPU from the GitHub release.
2. Extract `gproxy` or `gproxy.exe`.
3. Put it somewhere on your `PATH` or run it directly.

```bash
chmod +x ./gproxy
./gproxy --help
```

Release archives are built by the v2 release workflow for Linux, macOS, Windows,
Android, x86_64, and aarch64 targets. Linux release binaries are also used as
the input to the Docker image.

## Docker Image

The published image is `ghcr.io/leenhawk/gproxy`.

```bash
docker pull ghcr.io/leenhawk/gproxy:latest
docker run --rm -p 8787:8787 \
  -e GPROXY_ADMIN_PASSWORD=change-me-please \
  ghcr.io/leenhawk/gproxy:latest
```

The Docker image already contains a prebuilt native binary with the embedded
console. The image defaults to `GPROXY_HOST=0.0.0.0`,
`GPROXY_PORT=8787`, `GPROXY_PERSISTENCE=file`, and
`GPROXY_DATA_DIR=/app/data`.

See [Docker](/deployment/docker/) for persistent volumes, PostgreSQL/MySQL DSNs,
and tag selection.

## Build From Source

Use a source build when you are developing GPROXY or need a local build before a
release exists.

Prerequisites:

- A current stable Rust toolchain with edition 2024 support.
- Node.js and pnpm if you want the embedded console to match current
  `console/` sources. The release workflow uses Node 22 and pnpm 9.
- Platform libraries required by your Rust target.

Build the console first when its assets should be embedded:

```bash
cd console
pnpm install --frozen-lockfile
pnpm build
cd ..
```

Then build the binary from the repository root:

```bash
cargo build --release --bin gproxy
./target/release/gproxy --help
```

`pnpm build` creates `console/dist/` and then runs
`console/scripts/sync-to-embed.mjs`, which syncs the built files to
`assets/console/`. `rust-embed` compiles that directory into the native binary.

If you skip the console build, the gateway and admin APIs can still compile and
run, but `/console` may return `console assets not embedded`.

## Edge Bundles

Do not ask edge platforms to compile the Rust source. The supported edge path is
to upload a prebuilt bundle:

```text
build wasm in CI or on a machine with Rust -> generate platform bundle -> upload bundle
```

Release artifacts include platform zip files such as
`gproxy-edge-cloudflare.zip`, `gproxy-edge-netlify.zip`,
`gproxy-edge-supabase.zip`, `gproxy-edge-deno.zip`,
`gproxy-edge-eopages.zip`, and `gproxy-edge-appwrite-deno.zip`.

See [Edge Wasm Deployment](/deployment/edge/) for platform-specific commands and
runtime secrets.

## Next Steps

- Continue with [Quick Start](/getting-started/quick-start/) to boot a local
  instance.
- Read [Embedded Console](/guides/console/) before putting the native server
  behind a reverse proxy.
- Read [Migrating From v1 To v2](/deployment/v1-to-v2/) before pointing v2 at an
  existing v1 data directory.
