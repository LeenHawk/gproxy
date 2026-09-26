---
title: Installation
description: Build the GPROXY v4 binary from source, run it, find its data, and choose between the server, the desktop shell and the Workers host.
---

v4 has **no release pipeline**: no installers, no portable archives, no
published container image and no signed artifacts. There is one supported way
to get a binary, and it is to build one.

:::note[What happened to the download page]
v3 published installers for four platforms and a signed update manifest. None
of that was ported, and this site no longer describes it. The pages that did —
Downloads, Code signing and Container — were removed rather than rewritten
around machinery that does not exist.
:::

## Prerequisites

| Tool | Needed for |
| --- | --- |
| A stable Rust toolchain (edition 2024) | every crate |
| `wasm32-unknown-unknown` | the Workers host only |
| `webkit2gtk-4.1`, `gtk+-3.0`, `libsoup-3.0` (Linux) | the desktop shell only |
| Node.js LTS and pnpm | the console bundle only |

The tree was last built here with:

```text
rustc 1.98.0 (88d9e12ae 2026-08-18)
cargo 1.98.0 (797e8a9bc 2026-08-05)
```

## Build the Server

```sh
git clone https://github.com/LeenHawk/gproxy
cd gproxy
cargo build -p gproxy --release
```

The binary is `target/release/gproxy`.

```text
$ ./target/release/gproxy --version
gproxy 4.0.0-dev
```

The default feature set is a single-node SQLite instance with every channel
this repository implements: `channels`, `memory`, `fs`, `bundled-vocabulary`.

| Feature | Default | Adds |
| --- | --- | --- |
| `channels` | ✓ | all 25 channels. Name them one by one (`codex`, `kiro`, `openai`, …) for a binary that carries only the upstreams you use |
| `memory` | ✓ | the in-process cache |
| `fs` | ✓ | local file storage |
| `bundled-vocabulary` | ✓ | a fallback tokenizer vocabulary |
| `postgres` | | the PostgreSQL driver |
| `mysql` | | the MySQL driver |
| `redis` | | the shared cache a multi-instance deployment needs |
| `s3` | | S3-compatible file storage |

SQLite is always compiled in. A backend this build does not have is refused
**at startup**, naming the feature that would provide it, rather than at the
first request.

## Run It

```sh
./target/release/gproxy serve --data-dir ./data --port 8787
```

A first start creates the database and the schema, creates one administrator,
mints one gateway API key, and prints both **once**, to standard output:

```text
GPROXY first-run administrator (shown once)
  user:     admin
  password: p0Wsgf70xcFViWtn1UhZQ3msZSYHx2ZC
  api key:  sk-5hKlHHF0mtyw4pQewEj6pD2WZPkgH2vN-5lufKRtTdQ
Save these before closing this terminal; they are not stored in a form
this instance can show you again.
```

Save them. The password is argon2-hashed and the key is stored as a SHA-256
digest; the instance cannot show you either again.

The log goes to standard **error**, which is what keeps that block and
`gproxy export --out -` clean:

```text
WARN gproxy::serve: upstream credential secrets are stored UNENCRYPTED: no
     master key is configured. Set GPROXY_MASTER_KEY to 32 bytes as 64 hex
     characters or base64 …
INFO gproxy::serve: no console bundle is compiled into this binary and no
     directory was named, so /console answers 404. …
INFO gproxy::bootstrap: created the first administrator user="admin"
INFO gproxy::serve: gproxy is listening address=127.0.0.1:8787 revision=2 console=false
```

Both of those lines are true and deliberate. Read on.

### Set a master key before the first credential

With no master key, upstream credential secrets are stored **unencrypted**.
That is a supported deployment — the binary says so loudly once at startup —
but it is not one to keep by accident.

```sh
GPROXY_MASTER_KEY="$(openssl rand -hex 32)" \
  ./target/release/gproxy serve --data-dir ./data
```

32 bytes, as 64 hex characters or as base64. Setting it *after* credentials
already exist is a rotation, not an edit; see
[Configuration](/reference/configuration/#master-key-rotation).

### A source checkout has no console

`/console` is served from a bundle compiled into the binary, and **a source
checkout embeds nothing**. That is the intended state: `cargo build` produces a
binary whose console paths answer 404 rather than a blank page that looks like
a broken application, and the startup log says so.

To get one, build the console and point the binary at it:

```sh
cd console && pnpm install && pnpm build
GPROXY_CONSOLE_PATH=console/dist ./target/release/gproxy serve
```

A release build instead copies `console/dist` into
`crates/gproxy-host-axum/assets/web` before `cargo build`, and the bundle is
embedded.

## Where the Data Lives

Everything relative resolves against `--data-dir` (`GPROXY_DATA_DIR`,
default `data`):

| Path | What it is |
| --- | --- |
| `<data-dir>/gproxy.db` | the SQLite instance |
| `<data-dir>/` + `--file-storage-dir` | published bodies and downloaded vocabularies |

With `postgres` or `mysql` the directory still exists for the file storage.

`migrate` creates or incrementally synchronizes the schema and exits:

```text
$ ./target/release/gproxy migrate --data-dir ./data
INFO gproxy::instance: schema is up to date warnings=0
```

`serve` does this too. The separate command is for a deployment that runs
migrations as their own step, with one writer, before any instance starts —
which is what more than one instance over one database requires.

## The Desktop Shell

`gproxy-desktop` is a Tauri window over the same instance, new in v4. It is not
in the workspace's default members, because it pulls in webkit, gtk and about a
hundred and eighty crates that have nothing to say about the engine.

```sh
cargo run -p gproxy-host-tauri --bin gproxy-desktop
```

Two front doors, one instance:

- **the window**, over Tauri IPC, which carries the management and user
  surfaces. There is no authentication on it, deliberately: a message arrives
  only because this process's own webview sent it, so the channel is the proof.
- **`127.0.0.1:8787`**, a real axum host serving the **data plane only**, for
  Claude Code and the Codex CLI, which speak HTTP and cannot speak IPC. It
  **still demands a gateway key** — a loopback socket is not a trust boundary —
  and `/admin/api` and `/portal/api` answer 404 there, so the gateway key never
  doubles as an administrative one.

The default port is 8787, shared with the server. Change `port` in `gproxy.toml`
when running both on the same machine.

The master key is minted on first run and kept in the platform keychain
(Secret Service, the macOS Keychain, the Windows Credential Manager). It does
**not** fall back to a file — a key sitting next to the database it protects is
a longer path to the same plaintext. With no keychain the instance runs exactly
as the server does without `GPROXY_MASTER_KEY`, and says so.

## A Cloudflare Worker

The third host compiles the same router to wasm. See
[Edge (Cloudflare Workers)](/deployment/edge/).

## Next Steps

- [Quick Start](/getting-started/quick-start/) — a provider, a credential, a
  route and a request.
- [Configuration](/reference/configuration/) — every flag and every
  `GPROXY_*` variable.
- [Building from Source](/deployment/release-build/) — the quality gates and
  the other build targets.
