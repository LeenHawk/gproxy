---
title: Rust SDK
description: Current v2 Rust library surface, feature flags, and the boundary between internal modules and a published SDK.
---

The current v2 tree is a single Rust package named `gproxy`. It builds both:

- a native binary at `src/main.rs`;
- a library crate at `src/lib.rs` with `rlib` and `cdylib` crate types.

Unlike v1, this v2 checkout does not currently contain separate published
crates named `gproxy-sdk`, `gproxy-protocol`, `gproxy-channel`, or
`gproxy-engine`. Treat the `gproxy` library modules as the in-repo integration
surface unless/until a separate SDK package is added.

## Library modules

`src/lib.rs` exposes the same major modules used by the binary:

| Module | Responsibility |
| --- | --- |
| `protocol` | Provider-neutral operation taxonomy plus OpenAI, Claude, and Gemini wire types. |
| `transform` | Cross-protocol request/response transforms and stream adapters. |
| `channel` | Channel trait, built-in provider adapters, auth, credential refresh, request shaping, model list helpers, and channel routing tables. |
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
| `default` | Native default: memory cache, db and file persistence, wreq upstream client, local counting, and v1 migration. |
| `full` | Native convenience feature enabling all native backends. |
| `cache-memory` | In-process cache backend. |
| `cache-redis` | Redis cache backend for multi-instance cache/invalidation. |
| `persist-file` | Local JSON-file persistence backend. |
| `persist-db` | SeaORM database persistence backend. |
| `migrate-v1` | Legacy v1 SQLite migration reader and serve-path auto-migration hook. |
| `upstream-wreq` | Native HTTP upstream client. |
| `count-local` | Native local token-counting support through tokenizer dependencies. |
| `cache-libsql`, `cache-upstash`, `persist-libsql`, `upstream-fetch` | Wasm/edge backend gates. |
| `edge` | Umbrella for the wasm edge backend set. |

## Embedding boundary

The binary is intentionally thin: it parses CLI/env configuration, builds
persistence/cache/client/channel registry/state, optionally runs import/export
or migration/update subcommands, and then serves the HTTP router.

If you embed the library directly, you are responsible for wiring the same
pieces:

1. Build a `RuntimeConfig`.
2. Open a `PersistenceBackend`.
3. Build a `SecretCipher`.
4. Build a `CacheBackend`.
5. Build a `ChannelRegistry`.
6. Build an `AppState` and control-plane snapshot.
7. Call the HTTP router or lower-level pipeline functions.

That is not yet wrapped in a small public builder API.

## Protocol and operation taxonomy

The stable conceptual center of v2 is the operation taxonomy:

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

For production use, run the `gproxy` binary or one of the edge bundles. Use the
library surface for development, tests, custom deployments inside this
repository, or experiments where you can track internal API changes.
