---
title: Rust SDK
description: Current v3 Rust library surface, feature flags, and the boundary between internal modules and a published SDK.
---

The current v3 tree is a Rust workspace. The root package `gproxy` builds both:

- a native binary at `src/main.rs`;
- an `rlib` library crate at `src/lib.rs`. Edge build scripts explicitly request
  a `cdylib` when producing Wasm artifacts.

Five workspace members are published to crates.io under MIT, versioned in
lockstep with the `gproxy` release that ships them:

| Crate | What it is |
| --- | --- |
| [`gproxy-channel-api`](https://crates.io/crates/gproxy-channel-api) | Stable channel, metadata, login/refresh/usage, stream, host transport, and native compile-time registration contracts. |
| [`gproxy-protocol`](https://crates.io/crates/gproxy-protocol) | OpenAI/Claude/Gemini wire types, operation taxonomy, endpoint metadata. `serde` + `http` only, `wasm32`-clean. |
| `gproxy-protocol-macros` | Construction support for non-exhaustive protocol wire structs; normally used through `gproxy-protocol`. |
| [`gproxy-transform`](https://crates.io/crates/gproxy-transform) | Pairwise request/response/stream conversion between those three APIs. Pure and synchronous. |
| [`gproxy-tokenize`](https://crates.io/crates/gproxy-tokenize) | Offline token counting: tiktoken, Hugging Face vocabularies, character estimate. |

The `gproxy` root package itself is **not** published to crates.io. It is the
AGPL application, distributed as binaries, Docker images, and edge bundles. It
still has no published full `gproxy-sdk` or `gproxy-engine` counterpart. Use the
narrow `gproxy-channel-api` contract for external adapters and treat the
remaining root library modules as the in-repo integration surface.

Publishing is automated: pushing a `vX.Y.Z` tag runs
`scripts/publish-crates.sh` from the release workflow, which skips any
crate/version already on the registry.

## Library modules

`src/lib.rs` exposes the same major modules used by the binary:

| Module | Responsibility |
| --- | --- |
| `protocol` | Provider-neutral operation taxonomy plus OpenAI, Claude, and Gemini wire types. |
| `transform` | Cross-protocol request/response transforms and stream adapters. |
| `channel_api` | Re-export of the stable published channel extension contract. |
| `channel` | Host integration, built-in adapters, registry, metadata overlays, and root-owned channel helpers. |
| `native` | Native-only reusable standard CLI/bootstrap entrypoint for custom linked binaries. |
| `pipeline` | Request execution: auth, authz, route selection, failover, transforms, upstream execution, capture, and settlement. |
| `store` | Cache and persistence traits/backends. |
| `billing` and `usage` | Pricing, pending quota estimates, normalized usage extraction, and usage records. |
| `http` | Native Axum router/server pieces and wasm edge request handling. |
| `app` | Bootstrap, import/export, snapshots, v1 migration, invalidation, retention, and update status. |
| `crypto` | Password hashing and secret sealing/opening via `GPROXY_MASTER_KEY`. |
| `admin` and `api` | Cross-target admin guards and API helpers. |
| `selfupdate` | Native-only self-update implementation. |

These modules are useful for contributors and for embedding experiments, but
they should not be treated as a stable semver SDK contract yet.

## Feature flags

The package-level feature flags are backend-oriented:

| Feature | Purpose |
| --- | --- |
| `default` | Native default: memory cache, db persistence, wreq upstream client, local counting, v1 migration, and all native built-in channels. |
| `full` | Native convenience feature enabling all native backends and channels. |
| `cache-memory` | In-process cache backend. |
| `cache-redis` | Redis cache backend for multi-instance cache/invalidation. |
| `persist-db` | SeaORM database persistence backend. |
| `migrate-v1` | Legacy v1 SQLite migration reader and serve-path auto-migration hook. |
| `upstream-wreq` | Native HTTP upstream client. |
| `count-local` | Native local token-counting support through tokenizer dependencies. |
| `channels` | All built-in native channels. Individual adapters also have `channel-*` features. |
| `channels-edge` | Built-in channel subset compatible with Wasm/edge. |
| `cache-libsql`, `cache-upstash`, `persist-libsql`, `upstream-fetch` | Wasm/edge backend gates. |
| `edge` | Umbrella for the wasm edge backend set. |

`external-channels` is a `gproxy-channel-api` feature, not a root application
feature. It exposes the native registration slice and is enabled by the native
root dependency. The slice does not exist on `wasm32`.

## Embedding boundary

The official binary is intentionally thin. A custom native binary that only
adds compile-time channel crates should retain each crate and call the standard
entrypoint:

```rust
use my_gproxy_channel as _;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    gproxy::native::run_cli().await
}
```

This preserves the standard CLI, persistence, cache, client and channel
registry, migrations, subcommands, and HTTP server. The complete package and
source-identity requirements are documented in
[Adding a Channel](/guides/adding-a-channel/).

Deep embedding remains possible. If you bypass `run_cli()`, you are responsible
for wiring the same pieces:

1. Build a `RuntimeConfig`.
2. Open a `PersistenceBackend`.
3. Build a `SecretCipher`.
4. Build a `CacheBackend`.
5. Build a `ChannelRegistry`; use `with_builtin_and_linked()` if external
   registrations must be collected.
6. Build an `AppState` and control-plane snapshot.
7. Call the HTTP router or lower-level pipeline functions.

That is not yet wrapped in a small public builder API.

## Protocol and operation taxonomy

The stable conceptual center of v3 is the operation taxonomy:

- `Operation`: `list_models`, `get_model`, `count_tokens`,
  `generate_content`, `stream_generate_content`, `create_image`,
  `edit_image`, `create_embedding`, `compact_content`, and
  `create_conversation`;
- `OperationGroup`: models, count tokens, generate content, images,
  embeddings, compact, and conversation;
- `OperationKind`: either a provider family (`open_ai`, `claude`, `gemini`) or
  a content-generation wire kind (`open_ai_responses`,
  `open_ai_chat_completions`, `claude_messages`, `gemini_generate_content`).

Routing rules, transforms, endpoint synthesis, and settlement all build around
that taxonomy.

## Current recommendation

For normal production use, run the official `gproxy` binary, image, or edge
bundle. For a custom native provider adapter, use `gproxy-channel-api` and a
thin linked runner. Use the remaining root library surface only for deep
embedding, tests, or deployments that can track internal API changes.
