---
title: Architecture
description: The current gproxy v2 runtime architecture and request lifecycle.
---

gproxy v2 is a single Rust crate with two runtime surfaces:

- a native binary in `src/main.rs`, served by Axum and native upstream clients;
- a wasm library entry in `src/lib.rs` / `src/http/edge/`, used by edge platform
  bundles.

The crate is still layered. The important distinction from v1 is packaging, not
discipline: v2 keeps protocol types, transforms, request orchestration,
channels, storage, administration, and deployment boundaries separate inside
one repository.

## Repository Layout

```text
.
|-- Cargo.toml              # one crate: lib + bin
|-- src/
|   |-- main.rs             # native CLI, config, AppState, Axum server
|   |-- lib.rs              # shared module surface and wasm exports
|   |-- app/                # bootstrap, snapshots, import/export, v1 migration
|   |-- protocol/           # Operation taxonomy and provider wire models
|   |-- transform/          # operation-oriented protocol transforms
|   |-- process/            # provider rule-set compilation and application
|   |-- channel/            # upstream adapters and registry
|   |-- pipeline/           # request lifecycle orchestration
|   |-- http/               # native server, edge adapter, admin API dispatcher
|   |-- store/              # cache and persistence backends
|   `-- admin/ billing/ credentials/ health/ tokenize/ selfupdate/ usage/
|-- console/                # React console, built separately
|-- assets/console/         # generated console embed target
|-- deploy/                 # edge and platform packaging entries
|-- docs/                   # Starlight documentation website
`-- dev-docs/               # developer/source notes used as reference material
```

## Request Lifecycle

A normal generation request follows this path:

```text
HTTP request
  -> classify operation and inbound wire kind
  -> authenticate user API key
  -> normalize model name and alias
  -> resolve route or scoped provider
  -> enforce route permissions, rate limits, and quota admission
  -> select route member and credential
  -> transform protocol if inbound and upstream wire kinds differ
  -> apply provider rule sets
  -> prepare upstream request in channel
  -> send request through native or fetch client
  -> classify provider response
  -> fail over or settle usage
  -> shape response and transform back if needed
  -> log request, usage, quota deltas, and health state
```

`pipeline::execute` is the central orchestrator. It delegates to focused modules
for classification, auth, preprocessing, route resolution, authorization,
balance, transform, failover, and settlement.

## Operation-First Protocol Model

v2 avoids provider-family buckets as the primary documentation and code model.
The central concepts are:

| Type | Purpose |
| --- | --- |
| `OperationGroup` | Broad capability: models, count tokens, generate content, images, embeddings, compact, conversation. |
| `Operation` | Concrete action such as `ListModels`, `GenerateContent`, `CreateEmbedding`, `CompactContent`. |
| `OperationKind` | Provider wire shape for the operation, such as OpenAI Responses or Claude Messages. |
| `OperationKey` | `(operation, kind)`, used by routing rules and transforms. |

This is why content generation has more than one OpenAI kind: OpenAI Responses
and Chat Completions are different native wire shapes, not just labels.

## Transform, Process, Channel

Three layers are intentionally separate:

- **Transform** changes protocol shape by operation. It converts between OpenAI,
  Claude, and Gemini wire models when route execution requires it.
- **Process** applies configured request mutation rules after transform and
  before the upstream channel sees the request. The engine should remain
  permissive; provider-specific presets belong in configuration and the console
  unless the runtime truly needs a new primitive.
- **Channel** owns upstream access: endpoint, auth, request preparation, response
  disposition, optional stream decode, OAuth refresh, usage endpoints, and
  native TLS/HTTP2 profiles.

## AppState And Snapshots

Each request receives a cheap clone of `AppState`. The hot path reads an
`ArcSwap<ControlPlaneSnapshot>` containing provider, route, rule, and identity
records. Control-plane writes update persistence, rebuild the local snapshot,
and publish invalidation through the cache backend where the backend supports it.

Native instances can use memory or Redis cache plus file/db persistence. Edge
instances use fetch-compatible clients and platform-friendly persistence/cache
backends such as libSQL/Turso and REST-style shared stores.

## Runtime Boundaries

| Runtime | Boundary |
| --- | --- |
| Native | CLI/env config, Axum server, embedded console assets, native wreq client pool, optional self-update. |
| Edge | wasm entry, fetch adapter, platform-provided environment, no embedded console binary assets by default. |
| Console | React SPA in `console/`; build output is synced to `assets/console/` for native embedding. |
| Documentation | Starlight site in `docs/`; development/reference source notes live in `dev-docs/`. |

## Where To Go Next

- Configure upstreams in [Providers & Channels](/guides/providers/).
- Understand model-facing routing in [Models & Aliases](/guides/models/).
- Deploy native and edge builds in [Release Build](/deployment/release-build/) and
  [Edge Wasm](/deployment/edge/).
