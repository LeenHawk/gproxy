# gproxy-sdk

English | [简体中文](README.zh-CN.md)

The embeddable GPROXY handle. `gproxy-core` executes a request against
providers it is handed; it never writes configuration, never resolves a model
name and never learns that a peer changed something. This crate is the layer
that does: it assembles a `Core` out of default implementations, owns the
configuration writes that advance `settings.config_revision`, turns a login
into a credential row, resolves a model name to an execution plan, and keeps
every instance of a deployment on the same revision through the shared cache
and a durable revision poll.

```rust
use gproxy_sdk::{GproxyBuilder, SyncMode};

# async fn example() -> Result<(), gproxy_sdk::SdkError> {
let gproxy = GproxyBuilder::sqlite("gproxy.db")
    .await?
    .master_key([0u8; 32])
    .sync_mode(SyncMode::Background)
    .build()
    .await?;

for channel in gproxy.channels() {
    println!("{} ({})", channel.display_name, channel.id);
}
println!("serving revision {}", gproxy.revision().0);
# Ok(())
# }
```

`build()` opens nothing it was not given: it synchronizes the entity schema
(unless told not to), creates the global settings row, assembles the engine,
loads the first snapshot and — in `SyncMode::Background` — starts the
subscription and poll loops. A secret codec is never chosen implicitly:
`master_key` seals with AES-256-GCM, `plaintext_secrets` is the explicit
opt-out, and a build with neither is refused.

## Features

| Feature | Effect |
|---|---|
| `custom` / `codex` / `claudecode` / `claudeweb` | Compile a channel in and register it by default |
| `postgres` / `mysql` | Extra SeaORM drivers, native only. SQLite is always available natively |
| `libsql` | libSQL/Turso over the Hrana HTTP pipeline, every target |
| `d1` | Marker for a Cloudflare D1 binding, which wasm32 always has |
| `memory` (default) | Process-local `MemoryCache`, the default cache on native targets |
| `redis` | Redis/Valkey, for a deployment with more than one instance |
| `fs` (default) | Local filesystem object storage, native only |
| `s3` | S3/R2 object storage |
| `bundled-vocabulary` (default) | Ship DeepSeek's vocabulary for token estimation |
| `ts` | `ts-rs` declarations for the DTOs |

The default set compiles for `wasm32-unknown-unknown`, and so does `libsql`.
On wasm the cache defaults to `gproxy_store::StoreCache` and synchronization is
always manual: an isolate does not outlive its request, so `tick()` at the top
of a request is the whole mechanism.

Native builds use the `reqwest` transport. A host that wants another backend
(`wreq` for TLS emulation, `reqwest-native` for the Codex CLI's stack) enables
that feature on `gproxy-client` itself, or hands the builder a `ClientPool` of
its own.

## Synchronization

Two mechanisms, because neither alone is enough.

| | Carries | Fails by |
|---|---|---|
| `Invalidation` on the shared cache | "look again", within milliseconds | Losing messages: a failed publish, a lagging subscriber, a closed topic |
| `settings.config_revision` poll | The durable truth, every 30s by default | Being slow |

A notification never carries state: it says which revision exists, and an
instance reloads only when that is ahead of the one it serves. An unreadable
payload, or one about a credential this snapshot has never seen, reloads rather
than guesses. Reloads are serialized and monotonic — an older revision never
replaces a newer one, and a failed reload leaves the previous snapshot serving.

## What is not here

- **Identity.** Users, API keys, organizations, teams, permissions,
  subscriptions, rate limits and the OAuth issuer belong to the application
  layer, which reads the same durable revision through
  `gproxy_store::load_all_data`.
- **Downstream authentication.** Nothing here decides who a caller is; the
  handle is given a scope and a set of allowed providers and credentials.
- **A server.** No listener, no router, no middleware, no CLI. Native and edge
  hosts are built on top of this crate.
- **Execution.** Attempts, conversion, rewriting, observation, budgets and
  settlement are `gproxy-core`'s; this crate only decides what to hand it.
