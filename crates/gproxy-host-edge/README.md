# gproxy-host-edge

English | [简体中文](README.zh-CN.md)

The Cloudflare Workers host for GPROXY v4: a `fetch` handler over
[`gproxy-host-axum`](../gproxy-host-axum)'s router.

**There is no route table in this crate.** It mounts the same `axum::Router`,
with the same routes, that the native binary serves. A route the axum host
grows is a route this host serves, with no edit here.

```rust,ignore
#[event(fetch)]
async fn fetch(request: HttpRequest, env: Env, _ctx: Context)
    -> worker::Result<http::Response<axum::body::Body>>
{
    let instance = instance(&env).await?;   // per isolate, assembled once
    instance.tick().await;                  // catch up on other instances
    Ok(instance.router().call(request).await?)
}
```

That is the whole host. Everything else in here is assembly and configuration.

## Can axum's router really run in a Worker?

This was the open question, and the answer is yes — but not for free, and the
reasons are worth writing down because they are the reasons the alternative was
rejected.

**What was rejected.** v3 had `app.admin_dispatch(&parts, body)`, a string-keyed
dispatch table both hosts called. v4 deleted it, for a reason
[`gproxy-host-axum`'s `admin` module](../gproxy-host-axum/src/admin.rs) states:
a path no arm matched fell through and became a 404 that looked like a missing
row. The obvious replacement — this crate hand-writing forty-odd management
routes a second time — has the same failure with a longer fuse: the two lists
drift and nobody finds out until a console button 404s in production only.

**What works.** Three facts, each checked by compiling rather than by reading
documentation:

1. **axum builds for `wasm32-unknown-unknown`.** Its server halves are
   features, not the crate: `http1`/`http2` are hyper, `tokio` is `axum::serve`
   and `ConnectInfo`, `ws` is a hyper upgrade. With `default-features = false`
   and `json`, `query`, `matched-path`, `Router` and every extractor this
   gateway uses compile. `Router` itself is target-independent.
2. **`worker`'s `http` feature removes the adapter layer.** `#[event(fetch)]`
   then hands over an `http::Request<worker::Body>` and accepts any
   `http_body::Body<Data = Bytes>` back, and `Router<()>` implements
   `tower::Service<http::Request<B>>`. They meet directly. This is also why
   this crate does **not** port v3's hand-written `web_sys::Request`,
   `Response`, `ReadableStream` and header adapters: they were right for v3,
   which spoke web-sys itself, and here they would be a second copy of code
   Cloudflare maintains.
3. **`Send` was the real obstacle, and `send_wrapper` bridges it.** On wasm the
   engine below is `!Send` deliberately — a JS transport handle belongs to the
   isolate that made it, and `gproxy-client`'s `ClientBounds` is `Send + Sync`
   natively and empty on wasm — while axum wants
   `Router<S>: S: Clone + Send + Sync + 'static`
   (`axum-0.8.9/src/routing/mod.rs:89`), `Handler::Future: Send`
   (`handler/mod.rs:148`) and `Body::from_stream<S>: S: TryStream + Send`
   (`axum-core-0.5.6/src/body.rs:61`).

   Without a bridge, `gproxy_app::App<C>` is neither `Send` nor `Sync` on wasm,
   through `App<C>` → `Gproxy<C>` → `gproxy_sdk::handle::Inner<C>` →
   `gproxy_core::Core<C>` → `gproxy_client::pool::wasm::ClientPool`, which holds
   a `HashMap<_, Arc<Client>>` over `dyn OutboundClient` and an
   `Arc<dyn Fn(&ConnectionConfig, bool) -> Result<Client, Error>>`.

   Two changes fix it, both runtime-checked rather than asserted, and both
   sound because a Worker isolate is single-threaded: `gproxy_app::App` holds
   its handle in a `SendWrapper`, and `gproxy_host_axum::send` wraps each
   handler's body. A handler that forgets `send` is a **compile error on the
   wasm target naming that handler** — which is the enforcement, and the reason
   the wrapper is at the handler rather than a `cfg` on the router. A route
   that cannot compile for the edge cannot silently fail to exist here.

## What this host does not serve

| Surface | Why |
|---|---|
| websocket / realtime | a Worker upgrades by constructing a `WebSocketPair` and returning the client half in a `Response`'s `webSocket` field — a mechanism with no `http::Response` shape at all. A handshake is refused with `501`. |
| the embedded console | the bundle belongs in Workers Assets, in front of this Worker; see `wrangler.toml.example`. |

Everything else is the axum router's, unchanged: the data plane and its
channel service routes, the OAuth issuer, `/admin/api`, `/portal/api`,
`/publications/{id}` and `/healthz`.

Making realtime real needs two pieces, neither of them a routing question: a
`WebSocketPair` implementation of `gproxy_protocol::connection::WebSocket`, and
a way for a handler to hand the prepared JS `Response` back to the fetch entry
point. The second has a seam already — `http::Extensions` accepts a
`Send + Sync + Clone` value, and a `SendWrapper<web_sys::Response>` is all
three. It is deliberately not done here: an upgrade path that has never been
run against a real client is worth less than an honest refusal.

## Synchronization is one line

An isolate is not a process. It is created when there is traffic and destroyed
when there is not, and **nothing runs in it between requests** — a background
subscribe-and-poll loop is simply never polled. So the sdk is assembled with
`SyncMode::Manual` and the whole mechanism is `tick()` at the top of every
request: one read of `settings.config_revision`, and a reload of both snapshots
when it is newer than the one this isolate published.

A `tick` that fails is logged and swallowed. It is a *catch-up*: an isolate
that cannot read the revision right now serves the configuration it already
has, which is a previous revision rather than a wrong one. Refusing instead
would turn a momentary database hiccup into an outage for traffic that did not
need the unseen write.

The assembly is cached in a `OnceLock` for the isolate's life, because the
alternative is a full configuration load on every request. Two requests
arriving before the first assembly finishes will both assemble; the loser's is
dropped, costing one wasted cold start and nothing else, since D1 and libSQL
are both request-scoped HTTP with no connection to hold open.

## Configuration

Two levels, and no more. A Worker has no command line, no `.env` and no file.

1. **`GPROXY_CONFIG`** — one JSON document in exactly the `AppConfig` shape,
   the same type `gproxy.toml` holds for the native binary. Every field is
   optional and the whole variable is optional.
2. **the named secrets**, each overriding the corresponding field.

The split is not stylistic: `[vars]` live in `wrangler.toml`, in the
repository, in plain text, while `wrangler secret put` stores a value encrypted
and never shows it again. A master key belongs in the second place, so the only
way to keep it out of the first is to let a secret override the document.

| Binding | Kind | What it is |
|---|---|---|
| `GPROXY_CONFIG` | var | the `AppConfig` document, as JSON |
| `GPROXY_MASTER_KEY` | secret | 32 bytes, as 64 hex digits or base64 |
| `GPROXY_LIBSQL_TOKEN` | secret | the Turso bearer token, with a libSQL store |
| `GPROXY_S3_ACCESS_KEY_ID` | secret | with S3/R2 file storage |
| `GPROXY_S3_SECRET_ACCESS_KEY` | secret | with S3/R2 file storage |

Absent `GPROXY_CONFIG`, the defaults are the `DB` D1 binding and the
database-backed cache — the smallest thing that serves traffic. A document that
names *some* fields still gets those two defaults for the ones it left out,
because `AppConfig::default()` describes the native binary (a SQLite file, an
in-process cache) and a Worker can open neither.

The encoding of the master key is sniffed on the same rule the native binary
uses: exactly 64 hex digits is hex, anything else is base64. A 32-byte base64
key is 43 or 44 characters, so the two cannot be confused. Absent, secrets are
stored in plaintext and a warning is logged once per cold start.

### What assembly refuses, and why it refuses rather than answering wrongly

- **`cache: memory`.** Per isolate. A rate-limit counter kept there counts to
  one forever, and a login session written there is gone before the redirect
  comes back. Use `store` (the database as the cache) or `redis`.
- **`store: sqlite` / `store: url`.** A Worker cannot open a file or a TCP
  socket. Use `d1` or `libsql`.
- **`file_storage: fs`.** There is no filesystem. Use S3 — R2 speaks it.

### What a deployment must bind

`wrangler.toml.example` is a filled-in copy; the short version:

- a **D1 database**, as `[[d1_databases]]` with the binding `GPROXY_CONFIG`'s
  `store.binding` names (`DB` by default) — or a libSQL URL instead, with the
  crate built `--features libsql`;
- **`GPROXY_MASTER_KEY`**, unless plaintext secrets are acceptable;
- **`public_base_url`**, if any upstream needs a fetchable link for a
  published body;
- **S3/R2 and its two secrets**, for published bodies and downloaded
  vocabularies, with the crate built `--features s3`;
- **Workers Assets**, if the console is served from this deployment.

**Schema sync runs during cold-start assembly**, before settings, cache or
configuration are read. Concurrent requests in one isolate share that assembly.
D1 and libSQL use `Store::sync()` to add missing entity-defined objects; ordinary
requests only tick configuration revisions. Coordinate schema-changing deployments
with a single writer; explicit data/type migrations remain deployment work.

## Building

```sh
cargo install worker-build
CARGO_PROFILE_RELEASE_STRIP=none worker-build --release -- --no-default-features --features d1,custom,codex,claudecode
```

`worker-build` compiles to wasm, runs `wasm-bindgen`, optimizes with
`wasm-opt -Oz` and writes the JS shim `wrangler.toml`'s `main` points at.

**Size is a real constraint and this crate is close to it.** Measured on this
tree with `cargo build --release --target wasm32-unknown-unknown`, before
`wasm-bindgen` and `wasm-opt`:

| Build | raw | gzip |
|---|---|---|
| `--features channels` (all 25) | 25.9 MB | 7.7 MB |
| `--features d1,custom,codex,claudecode` | 24.8 MB | 7.4 MB |

Cloudflare's limit is on the compressed bundle — 3 MB on the free plan, 10 MB
on paid. `wasm-opt -Oz` and `wasm-bindgen`'s garbage collection cut a
substantial part of this and are not applied in those numbers; **no deployment
was measured, because this tree has no Cloudflare account and no wrangler
toolchain.** A deployment must check its own bundle. Naming channels one by one
instead of `channels` helps a little; `bundled-vocabulary` is off by default
because it is hundreds of kilobytes of tokenizer.

## Features

| Feature | What it adds |
|---|---|
| `d1` *(default)* | the Cloudflare D1 store |
| `libsql` | a libSQL/Turso store over the fetch transport |
| `s3` | S3/R2 object storage for published bodies and vocabularies |
| `bundled-vocabulary` | DeepSeek's vocabulary, for models with no catalogue file |
| `channels` *(default)* | every channel this repository implements; name them one by one for a smaller binary |

## What is tested, and what is not

`config.rs` is plain data and its tests run on the host target
(`cargo test -p gproxy-host-edge`): the document shape, the edge defaults, the
secret overlay, the master-key encoding sniff, and each of the four
misconfigurations above.

Everything else in this crate is behind `cfg(target_arch = "wasm32")` and is
verified by compiling — `cargo clippy -p gproxy-host-edge --target
wasm32-unknown-unknown --lib -- -D warnings` and a real `cargo build` artifact.
**Unexercised:** the fetch entry point, the isolate assembly, the D1 binding
lookup, the libSQL transport and `tick()` have never been run, because running
them needs a Workers runtime. The route table underneath them is the native
host's and is covered by its tests.

## First administrator

Before deploying, run `wrangler secret put GPROXY_ADMIN_PASSWORD` with an initial password of at least 8 characters. First assembly creates `admin` only if the users table is empty; `GPROXY_ADMIN_USER` changes the name. Any existing user disables bootstrap, so passwords are never reset and the initial secret may be removed. Sign in at `/console/` and create a gateway API key. A conditional database insert prevents duplicate creation across isolates.
