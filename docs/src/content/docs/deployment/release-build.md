---
title: "Building from Source"
description: "Build every GPROXY v4 target from source — the server, the desktop shell, the Worker and the console — and run the quality gates CI runs."
---

Building from source is the only way to get v4. There is **no release
pipeline**: no installers, no portable archives, no published container image,
no signed update manifest and no code-signing.

That is a statement about v4's current state, not a policy. v3 had all of it,
and none of it was ported; the pages that described it were removed rather than
rewritten around machinery that does not exist. The `deploy/` and `scripts/`
directories in the repository still hold v3's pipeline and **do not build v4**
— the container Dockerfile, for instance, asks for a binary target that moved
crates.

## Prerequisites

| Tool | Needed for |
| --- | --- |
| A stable Rust toolchain (edition 2024) | every crate |
| `wasm32-unknown-unknown` | the Workers host, and the CI check |
| `webkit2gtk-4.1`, `gtk+-3.0`, `libsoup-3.0` (Linux) | the desktop shell |
| Node.js LTS and pnpm | the console and this documentation site |
| `worker-build` | the Workers bundle |

## The Workspace

Sixteen crates. `cargo check`, `cargo test` and `cargo clippy` with no `-p` and
no `--workspace` build the **default members**, which is everything except the
desktop shell.

That exclusion is not a demotion and it is not about incremental builds: with a
warm target directory the two selections are within half a second of each
other. It is about the **cold** one. The desktop shell brings wry, webkit, gtk
and their `-sys` crates — about a hundred and eighty extra third-party crates
that have nothing to say about the engine, and that a Linux box without the
development headers cannot build at all.

`--workspace` builds it, and CI does.

## The Server

```sh
cargo build -p gproxy --release
./target/release/gproxy --version
```

```text
gproxy 4.0.0-dev
```

The default features are `channels`, `memory`, `fs` and `bundled-vocabulary` —
a single-node SQLite instance with every channel compiled in. Name channels one
by one for a binary that carries only the upstreams a deployment uses:

```sh
cargo build -p gproxy --release --no-default-features \
  --features memory,fs,codex,claudecode,openai,custom
```

Add `postgres`, `mysql`, `redis` or `s3` as the deployment needs. SQLite is
always compiled in, and a backend this build does not have is refused at
startup naming the feature that would provide it.

## The Console

```sh
cd console
pnpm install --frozen-lockfile
pnpm build
```

A release build copies `console/dist` into
`crates/gproxy-host-axum/assets/web` **before** `cargo build`, and the bundle
is embedded with `rust-embed`.

**A source checkout embeds nothing**, and that is the intended state:
`cargo build` produces a binary whose console paths answer 404 rather than a
blank page that looks like a broken application, and the startup log says so.

For development against a Vite build, point the binary at a directory instead:

```sh
GPROXY_CONSOLE_PATH=console/dist ./target/release/gproxy serve
```

The TypeScript types the console is written against are **generated from
Rust**, never written by hand, from two crates into two directories:

```sh
GPROXY_TS_OUT=console/src/generated/sdk cargo test -p gproxy-sdk --features ts export_types
GPROXY_TS_OUT=console/src/generated/app cargo test -p gproxy-app --features ts export_types
```

Without the variable each test returns immediately and writes nothing, so
`cargo test --all-features` stays hermetic and a generated directory is only
ever rewritten on purpose. The two go to **separate** directories because the
export wipes its output first, and two crates sharing one would erase each
other.

## The Desktop Shell

```sh
cargo run -p gproxy-host-tauri --bin gproxy-desktop
```

One process, one instance, two front doors: the window over Tauri IPC for the
management and user surfaces, and a real axum host on `127.0.0.1:7071` serving
the **data plane only**, for the CLIs that speak HTTP and cannot speak IPC.

The test suite drives the whole arrangement on a machine with **no display
server**, because almost everything is in the library and the binary only opens
a window.

```sh
cargo check  -p gproxy-host-tauri
cargo clippy -p gproxy-host-tauri --all-targets --all-features -- -D warnings
cargo test   -p gproxy-host-tauri
```

This crate carries no auto-update, no launch-at-login and no tray icon. Every
one of those is a decision about how software is *distributed* rather than
about what it does, and adding them first would mean maintaining an update
channel for an application with no users.

## The Worker

```sh
cargo install worker-build
worker-build --release -- --no-default-features --features d1,custom,codex,claudecode
```

See [Edge (Cloudflare Workers)](/deployment/edge/) for the bindings, the
configuration document and the size constraint that makes naming channels one
by one worth doing.

## The Quality Gates

Exactly what CI runs:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check --workspace --target wasm32-unknown-unknown
```

The last one is not optional decoration. `gproxy-app` and the axum router both
build for wasm, which is what lets the Workers host mount the same router
instead of writing the route table a second time — and a handler that forgets
its `Send` bridge is **a compile error on that target naming the handler**. The
check is the enforcement.

A lint finding gets a code change, not an `#[allow]`.

Per-crate, while working on one:

```sh
cargo test   -p gproxy-channel --all-features
cargo clippy -p gproxy-channel --all-features --target wasm32-unknown-unknown --lib -- -D warnings
cargo test   -p gproxy-host-axum
cargo test   -p gproxy-store -p gproxy-seaorm
```

Every `gproxy-host-axum` integration test builds the **real router** over an
in-memory instance, with only the upstream scripted. A test that called a
handler function directly would skip the part being tested.

Two suites bind a real loopback port because they cannot be faked: a websocket
round trip, because the in-process service harness never produces hyper's
upgrade extension; and a client disconnect, because every HTTP client in the
tree drains a body before handing it over, so that suite types the request out
over a raw socket and hangs up by dropping it.

## This Documentation Site

```sh
cd docs
pnpm install --frozen-lockfile
pnpm check
pnpm build
```

Astro Starlight, deployed to Cloudflare Pages by CI. `pnpm check` validates the
notification feed the site also hosts.

`scripts/check-docs.sh` is the structural check: sidebar slugs against pages,
English and Chinese parity, frontmatter, forbidden references and oversized
pages.

## What Is Not Here

No `cargo publish`. Nothing in the workspace goes to a registry, so embedding
means a git or path dependency — see
[Embedding the Core](/reference/embedding/). The public surface is not stable.
