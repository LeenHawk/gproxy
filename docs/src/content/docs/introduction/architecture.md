---
title: Architecture
description: The current GPROXY v2 runtime architecture and request lifecycle.
---

GPROXY v2 is a single Rust crate with two runtime surfaces:

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

## Stable Code Reference Index

Some source comments use unqualified `§` labels inherited from the original v2
design notes. Those labels are stable identifiers, not the ordinal positions of
headings on this page. Use this index to resolve them; do not renumber them when
this page is reorganized. Qualified references such as `RFC 7230 §6.1` or a
specific document path refer to that external document instead.

| Stable label | Named architecture topic |
| --- | --- |
| `§3.2` | Passive health and circuit breakers. |
| `§3.3` | Per-credential RPM/TPM admission budgets. |
| `§4` | Shared admin API contracts and DTOs. |
| `§5` | End-to-end request lifecycle and pipeline boundaries. |
| `§6`, `§6.1` | Operation-first transforms and provider rule processing. |
| `§6.3` | Channel registry, local operations, and request orchestration. |
| `§6.4` | Upstream disposition and bounded failover. |
| `§7.2` | Control-plane snapshots, hot reload, and invalidation. |
| `§7.4` | Effective upstream proxy, TLS fingerprint, and HTTP transport. |
| `§8` | Control-plane persistence and instance settings. |
| `§8-A` | Routes, route members, aliases, and exposed provider models. |
| `§8-B` | Providers, credentials, model variants, and transform dispatch. |
| `§8-B2` | Routing rules and provider rule sets. |
| `§8-C` | Identity-scoped permissions, rate limits, and quotas. |
| `§8-D` | Usage records, wire logs, capture, and retention. |
| `§8-E` | Runtime settings and usage/log feature toggles. |
| `§9` | Console build, embedding, and edge asset packaging. |
| `§13` | Cache behavior, invalidation, and edge configuration refresh. |
| `§14.1` | Secret envelope encryption and decrypt-at-use. |
| `§14.2` | First-boot admin, password hashing, and sessions. |
| `§14.3` | Secret redaction and security-sensitive runtime settings. |
| `§14.5` | OAuth login, refresh, and credential usage lifecycle. |
| `§15`, `§15.1`, `§15.2`, `§15.3` | Observability: request IDs, tracing, metrics, and latency. |
| `§16.1`, `§16.2`, `§16.3` | Runtime hardening: graceful drain, overload/timeout bounds, and health-edge persistence. |
| `§17` | Normalized usage, billing, quota admission, and settlement. |
| `§18` | Control-plane import and export. |
| `§19` | Native self-update lifecycle. |
| `§19.2`, `§19.3`, `§19.4` | Signed manifests, release channels, rollback guards, and update policy. |
| `§19.5`, `§19.6`, `§19.6.1`, `§19.6.2` | Download/staging, binary swap, supervisor restart, and direct re-exec. |
| `§19.7`, `§19.8`, `§19.10` | Data compatibility, rollback artifacts, and update admin/status safety. |

## Where To Go Next

- Configure upstreams in [Providers & Channels](/guides/providers/).
- Understand model-facing routing in [Models & Aliases](/guides/models/).
- Deploy native and edge builds in [Release Build](/deployment/release-build/) and
  [Edge Wasm](/deployment/edge/).
