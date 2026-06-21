---
title: What is GPROXY v2?
description: A high-level overview of the GPROXY v2 rewrite and what it is designed to do.
---

**GPROXY v2** is the rewrite of the GPROXY LLM gateway. It keeps the original
goal from v1: one HTTP entry point for many LLM providers, with routing,
credentials, user API keys, policy, usage accounting, and a browser console.
The implementation shape is different: v2 is one Rust crate that can build a
native server binary and a wasm library for edge runtimes.

The v2 design is intentionally operation-oriented. Protocol behavior is grouped
by capability, such as model listing, token counting, content generation,
embeddings, images, compacting, and conversations. Provider families still
matter at the wire boundary, but they are not the organizing unit for routing or
transforms.

## What v2 Is Good At

- **One gateway for multiple providers.** Providers are configured as channel
  instances with settings, credentials, health state, optional TLS fingerprints,
  and rule sets.
- **OpenAI, Claude, and Gemini-compatible traffic.** v2 classifies inbound
  requests by operation and wire kind, then either keeps same-protocol traffic
  light or transforms requests into the selected provider-native format.
- **Multi-tenant access.** Users, organizations, teams, user API keys, route
  permissions, rate limits, and quotas are part of the control plane.
- **Operational routing.** A public model name can resolve to an aggregate
  route, route members, upstream model ids, and credentials. Failover and
  health state live around this route execution path.
- **Native and edge deployments.** The native binary uses Axum and wreq. The
  wasm build uses fetch-compatible transports with libSQL/Turso and Upstash-style
  backends where the platform supports them.
- **Embedded administration.** The React console is built separately and can be
  embedded into the native binary or served as static files next to the API.

## What Changed From v1

v1 was organized as a Cargo workspace with app crates, server crates, and SDK
crates. v2 collapses that into one crate with clear module boundaries under
`src/`. This is not a downgrade in layering; it is a packaging decision that
keeps the native binary, wasm library, and shared runtime code in one place.

The most important conceptual changes are:

| Area | v1 shape | v2 shape |
| --- | --- | --- |
| Repository | Workspace with apps, crates, and SDK packages | One crate with native and wasm outputs |
| Protocol matrix | Provider-family language appears in more places | Operation / OperationGroup first |
| Config flow | TOML/database-oriented v1 control plane | Import/export snapshots plus persistence backends |
| Console | Separate frontend embedded at build time | React console remains separate but is synced into `assets/console` |
| Edge | Not the primary runtime shape | First-class wasm library and platform bundles |

## Core Concepts

| Concept | Meaning in v2 |
| --- | --- |
| Provider | A configured upstream adapter: channel id, settings, credentials, optional proxy and TLS behavior. |
| Channel | The code that prepares provider-native requests and classifies provider-native responses. |
| Operation | A capability such as `GenerateContent`, `ListModels`, `CreateEmbedding`, or `CountTokens`. |
| Route | A public model entry that selects one or more provider/upstream model members. |
| Alias | A user-facing model name that maps to a route. |
| Rule set | Ordered request mutation rules applied after protocol transform and before channel send. |
| Snapshot | The hot-path control-plane view read by request execution. |
| Cache backend | Ephemeral/shared coordination, sessions, counters, invalidation, and locks. |
| Persistence backend | Durable control-plane records, logs, usage, audit, and metrics. |

## What It Is Not

GPROXY v2 is not a model host; it does not run inference. It is not a generic
reverse proxy; it understands LLM protocol operations. It is also not a managed
hosted SaaS console; the embedded console is part of your deployment and should
sit behind your own network and operational controls.

## Next Steps

- Read the current-state [Architecture](/introduction/architecture/).
- Install and run v2 from [Installation](/getting-started/installation/).
- Import a local development snapshot in [Quick Start](/getting-started/quick-start/).
- Migrate an existing v1 deployment with [v1 to v2 Migration](/deployment/v1-to-v2/).
