---
title: "Edge (Cloudflare Workers)"
description: "Run GPROXY v4 as a Cloudflare Worker over the same axum router the native binary serves: bindings, the two configuration levels, what it refuses, and the size constraint."
---

`gproxy-host-edge` is a Cloudflare Workers `fetch` handler over
`gproxy-host-axum`'s router.

**There is no route table in this crate.** It mounts the same `axum::Router`,
with the same routes, that the native binary serves. A route the native host
grows is a route this host serves, with no edit here.

```rust
#[event(fetch)]
async fn fetch(request: HttpRequest, env: Env, _ctx: Context)
    -> worker::Result<http::Response<axum::body::Body>>
{
    let instance = instance(&env).await?;   // per isolate, assembled once
    instance.tick().await;                  // catch up on other instances
    Ok(instance.router().call(request).await?)
}
```

That is the whole host. Everything else is assembly and configuration.

This is new in v4. v3's edge host was a hand-written dispatch over
`web_sys::Request`; v4 deleted it, because the alternative — this crate
hand-writing forty-odd management routes a second time — has a failure with a
long fuse: the two lists drift, and nobody finds out until a console button
404s in production only.

## What It Does Not Serve

| Surface | Why |
| --- | --- |
| websocket / realtime | A Worker upgrades by constructing a `WebSocketPair` and returning the client half in a response's `webSocket` field — a mechanism with **no `http::Response` shape at all**. A handshake is refused with `501`. |
| the embedded console | The bundle belongs in Workers Assets, in front of this Worker. |

Everything else is the axum router's, unchanged: the data plane and its channel
service routes, the OAuth issuer, `/admin/api`, `/portal/api`,
`/publications/{id}` and `/healthz`.

Making realtime real needs two pieces, neither of them a routing question: a
`WebSocketPair` implementation of the protocol's socket type, and a way for a
handler to hand the prepared JS response back to the fetch entry point. The
second has a seam already. It is **deliberately not done**: an upgrade path
that has never been run against a real client is worth less than an honest
refusal.

## Size Is a Real Constraint

Measured on this tree with a plain release build for
`wasm32-unknown-unknown`, **before** `wasm-bindgen` and `wasm-opt`:

| Build | raw | gzip |
| --- | --- | --- |
| `--features channels` (all 25) | 25.9 MB | 7.7 MB |
| `--features d1,custom,codex,claudecode` | 24.8 MB | 7.4 MB |

Cloudflare's limit is on the **compressed** bundle: 3 MB on the free plan,
10 MB on paid. `wasm-opt -Oz` and `wasm-bindgen`'s garbage collection cut a
substantial part of those numbers and are not applied in them.

**No deployment was measured, because this tree has no Cloudflare account and
no wrangler toolchain.** A deployment must check its own bundle.

Naming channels one by one instead of `channels` helps a little.
`bundled-vocabulary` is off by default because it is hundreds of kilobytes of
tokenizer.

## Building

```sh
cargo install worker-build
CARGO_PROFILE_RELEASE_STRIP=none worker-build --release -- --no-default-features --features d1,custom,codex,claudecode
```

`worker-build` compiles to wasm, runs `wasm-bindgen`, optimizes with
`wasm-opt -Oz`, and writes the JS shim `wrangler.toml`'s `main` points at. It
is **not optional**: an unoptimized build is several times over the limit.

| Feature | What it adds |
| --- | --- |
| `d1` *(default)* | the Cloudflare D1 store |
| `libsql` | a libSQL/Turso store over the fetch transport |
| `s3` | S3/R2 storage for published bodies and vocabularies |
| `bundled-vocabulary` | DeepSeek's vocabulary |
| `channels` *(default)* | all 25; name them one by one for a smaller binary |

## Configuration

Two levels, and no more. A Worker has no command line, no `.env` and no file.

1. **`GPROXY_CONFIG`** — one JSON document in exactly the shape
   `gproxy.toml` holds for the native binary. Every field is optional and so is
   the whole variable.
2. **the named secrets**, each overriding the corresponding field.

The split is not stylistic. `[vars]` live in `wrangler.toml`, in the
repository, in plain text, while `wrangler secret put` stores a value encrypted
and never shows it again. A master key belongs in the second place, so the only
way to keep it out of the first is to let a secret override the document.

| Binding | Kind | What it is |
| --- | --- | --- |
| `GPROXY_CONFIG` | var | the configuration document, as JSON |
| `GPROXY_MASTER_KEY` | secret | 32 bytes, as 64 hex digits or base64 |
| `GPROXY_LIBSQL_TOKEN` | secret | the Turso bearer token, with a libSQL store |
| `GPROXY_S3_ACCESS_KEY_ID` | secret | with S3/R2 file storage |
| `GPROXY_S3_SECRET_ACCESS_KEY` | secret | with S3/R2 file storage |

Absent `GPROXY_CONFIG`, the defaults are the `DB` D1 binding and the
database-backed cache — the smallest thing that serves traffic. A document that
names *some* fields still gets those two for the ones it left out, because the
shared type's own defaults describe the native binary (a SQLite file, an
in-process cache) and a Worker can open neither.

`host`, `port`, `data_dir` and `console` describe a process with a socket and a
filesystem. They are accepted — it is one shared type — and never read.

```toml
[vars]
GPROXY_CONFIG = """
{
  "store": { "kind": "d1", "binding": "DB" },
  "cache": { "kind": "store" },
  "public_base_url": "https://gproxy.example.workers.dev",
  "cors_origins": ["https://gproxy.example.workers.dev"],
  "trusted_proxies": [],
  "session_ttl_secs": 2592000,
  "oauth": { "access_ttl_secs": 3600, "cli_client_ids": [] }
}
"""
```

The master key's encoding is sniffed on the same rule the native binary uses:
exactly 64 hex digits is hex, anything else is base64. A 32-byte base64 key is
43 or 44 characters, so the two cannot be confused. Absent, secrets are stored
in plaintext and a warning is logged once per cold start.

### What assembly refuses, and why it refuses rather than answering wrongly

- **`cache: memory`.** It is per isolate. A rate-limit counter kept there
  counts to one forever, and a login session written there is gone before the
  redirect comes back. Use `store` (the database as the cache) or `redis`.
- **`store: sqlite` / `store: url`.** A Worker cannot open a file or a TCP
  socket. Use `d1` or `libsql`.
- **`file_storage: fs`.** There is no filesystem. Use S3 — R2 speaks it.

## What a Deployment Must Bind

`crates/gproxy-host-edge/wrangler.toml.example` is a filled-in copy. The short
version:

- a **D1 database**, as `[[d1_databases]]` with the binding
  `GPROXY_CONFIG`'s `store.binding` names (`DB` by default) — or a libSQL URL
  instead, with the crate built `--features libsql`;
- **`GPROXY_MASTER_KEY`**, unless plaintext secrets are acceptable;
- **`public_base_url`**, if any upstream needs a fetchable link for a published
  body;
- **S3/R2 and its two secrets**, for published bodies and downloaded
  vocabularies, with the crate built `--features s3`;
- **Workers Assets**, if the console is served from this deployment.

```toml
[[d1_databases]]
binding = "DB"
database_name = "gproxy"
database_id = "00000000-0000-0000-0000-000000000000"

[assets]
directory = "./public"
binding = "ASSETS"
not_found_handling = "single-page-application"
run_worker_first = ["/*", "!/", "!/console", "!/console/*"]
```

`deploy/cloudflare` is this layout, and a release's
`gproxy-edge-cloudflare.zip` is it already built. The console's bundle sits in
`public/console/`, its document is copied to `public/index.html` for the
single-page fallback, and `public/_headers` gives it the security headers the
native host sends. Everything except the console reaches the Worker first; an
allowlist of API paths would miss the provider mounts, whose prefixes the
configuration chooses. The console never wakes an isolate, and the wasm binary
stays under the size limit.

**Migrations are not the Worker's.** It runs under traffic, and DDL under
traffic is how two isolates deadlock a migration, so the builder never touches
a schema. Run `wrangler d1 migrations apply` before the deployment.

```sh
wrangler d1 create gproxy
wrangler d1 migrations apply gproxy --remote
wrangler secret put GPROXY_MASTER_KEY
wrangler deploy
```

## Synchronization Is One Line

An isolate is not a process. It is created when there is traffic and destroyed
when there is not, and **nothing runs in it between requests** — a background
subscribe-and-poll loop is simply never polled.

So the handle is assembled in manual mode and the whole mechanism is `tick()`
at the top of every request: one read of the configuration revision, and a
reload of both snapshots when it is newer than the one this isolate published.

A `tick` that fails is logged and swallowed. It is a *catch-up*: an isolate
that cannot read the revision right now serves the configuration it already
has, which is a previous revision rather than a wrong one. Refusing would turn
a momentary database hiccup into an outage for traffic that did not need the
unseen write.

The assembly is cached for the isolate's life, because the alternative is a
full configuration load on every request. Two requests arriving before the
first assembly finishes will both assemble; the loser's is dropped, costing one
wasted cold start and nothing else, since D1 and libSQL are both
request-scoped HTTP with no connection to hold open.

## How axum Runs in a Worker

Three facts, each checked by compiling rather than by reading documentation.

1. **axum builds for `wasm32-unknown-unknown`.** Its server halves are Cargo
   features, not the crate: `http1`/`http2` are hyper, `tokio` is the serve
   function and the connection info, `ws` is a hyper upgrade. With those off,
   the router and every extractor this gateway uses compile.
2. **The `worker` crate's `http` feature removes the adapter layer.** The fetch
   event hands over an `http::Request` and accepts an `http_body::Body` back,
   and the router implements the tower service over exactly those. They meet
   directly — which is also why this crate does **not** port v3's hand-written
   web-sys adapters: they were right for v3, and here they would be a second
   copy of code Cloudflare maintains.
3. **`Send` was the real obstacle.** On wasm the engine is `!Send`
   deliberately — a JS transport handle belongs to the isolate that made it —
   while axum wants `Send + Sync` state and `Send` handler futures.

Two runtime-checked bridges fix the third, both sound because a Worker isolate
is single-threaded: the product layer holds its handle in a `SendWrapper`, and
the axum host wraps each handler's body. **A handler that forgets the wrapper
is a compile error on the wasm target naming that handler** — which is the
enforcement, and the reason it sits at the handler rather than behind a `cfg`
on the router. A route that cannot compile for the edge cannot silently fail to
exist there.

## What Is Tested, and What Is Not

The configuration half is plain data and its tests run on the host target:

```sh
cargo test -p gproxy-host-edge
```

They cover the document shape, the edge defaults, the secret overlay, the
master-key encoding sniff, and each of the four refusals above.

Everything else is behind `cfg(target_arch = "wasm32")` and is verified by
compiling:

```sh
cargo clippy -p gproxy-host-edge --target wasm32-unknown-unknown --lib -- -D warnings
```

**Unexercised:** the fetch entry point, the isolate assembly, the D1 binding
lookup, the libSQL transport and `tick()` have never been run, because running
them needs a Workers runtime. The route table underneath them is the native
host's and is covered by its tests.
