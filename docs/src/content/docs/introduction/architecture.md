---
title: Architecture
description: How GPROXY v3 is built — one embeddable core, interchangeable hosts, a single settlement funnel, pairwise protocol transforms, and one schema.
---

GPROXY v3 is a ground-up Rust rewrite of the v2 gateway. The job is the same:
one API-key surface in front of many LLM providers, with pooled upstream
credentials, routing and failover, quota admission, usage accounting and
billing, protocol conversion between wire formats, and emulated vendor control
planes so official CLIs run against pooled credentials. What changed is the
shape. In v2, wiring a new feature touched up to 63 files across 8 layers, and
roughly half of every feature diff was plumbing. v3 is built so that a feature
lands where its facts live and nowhere else.

This page is the map. It describes the crates, the boundary between the core
and its hosts, the execution model, and the rules that keep the structure from
drifting back.

## One Core, Many Hosts

`gproxy-core` is an embeddable library: channels, credential lifecycle,
protocol transforms, and the execution pipeline. Hosts adapt it to a runtime:

| Host | Runtime | Notes |
| --- | --- | --- |
| `gproxy-host-axum` | Native binary (`gproxy`) | Tokio + axum listener, embedded console, announcements, autostart, self-update. |
| `gproxy-host-edge` | Fetch-based platforms | Cloudflare Workers, Deno Deploy, Netlify Edge; compiled to `wasm32-unknown-unknown`. |
| Your application | Direct embedding | Links `gproxy-core` (or `gproxy-app`) and calls the same execute surface. |

Every host calls the same public API. There is no private entry into the
engine for the gateway's own HTTP layer: in v2 the Codex service surface was
a 1,700-line parallel reimplementation of the pipeline that forwarded real
inference upstream without settlement, and the unbilled traffic was only
found in v2.9.1. In v3 that class of bug cannot be written, because the core
is the only path to an upstream.

## Crate Graph

```text
   gproxy-host-axum        gproxy-host-edge         your application
   native listener         fetch runtimes           direct embedding
          \                     /                          |
           +-----> gproxy-app <+                           |
                   bootstrap · config · snapshot · wiring  |
             /       |         |          \                |
  gproxy-admin  gproxy-store  gproxy-upstream  gproxy-tokenize
  DTOs (ts-rs)  4 SQL dialects  HTTP/WS client  token counting
  pure dispatch + cache backends  TLS profiles
             \       |         /                           |
              v      v        v                            v
              gproxy-core  <-------------------------------+
              execution engine · host traits · boundary types
                 |                    |
          gproxy-channels       gproxy-transform
          28 upstream adapters  pairwise + envelope adapters
                 |                    |
          gproxy-channel-api          |
          channel contract            |
                 \                   /
                  gproxy-protocol
                  wire models · operation taxonomy · OperationSpec
```

Arrows only point downward. Hosts depend on the core, never the reverse, and
the core never gains a listener, a router, a UI, or a concrete store.
`gproxy-app` is the first embedder: it wires store backends, the admin plane,
the upstream transport, and the tokenizer around the core, and hosts adapt
`gproxy-app` to a runtime. A second embedder can link `gproxy-core` alone and
provide the host traits itself.

| Crate | Responsibility |
| --- | --- |
| `gproxy-protocol` | Typed OpenAI, Claude, and Gemini wire models; the operation taxonomy; `OperationSpec`, the one declaration of every operation. |
| `gproxy-channel-api` | The `Channel` contract: descriptor, capability rows, routing defaults, request preparation, stream decoding, usage extraction, login and refresh, service-surface tables, client fingerprint data. |
| `gproxy-transform` | Pairwise request, response, and stream conversions between wire formats, plus envelope adapters. |
| `gproxy-channels` | The built-in adapters, one directory per channel. |
| `gproxy-core` | The engine: classification, authentication, resolution, admission, transform slot, failover, the settlement funnel, service surfaces; the host traits; framework-free boundary types. |
| `gproxy-upstream` | The canonical `UpstreamTransport`: native HTTP/WebSocket client with TLS fingerprint profiles, and a fetch client for wasm. |
| `gproxy-store` | One schema catalog and one query layer for SQLite, libSQL, PostgreSQL, and MySQL; cache backends. |
| `gproxy-tokenize` | Offline token counting: tiktoken vocabularies, Hugging Face tokenizers, a bundled DeepSeek vocabulary, and a character estimate. |
| `gproxy-admin` | Admin and portal DTOs (TypeScript generated with `ts-rs`) and the pure `(state, request) → response` dispatch. |
| `gproxy-app` | Bootstrap: configuration, store and cache selection, the control-plane snapshot, channel registration, `App::start`. |
| `gproxy-host-axum`, `gproxy-host-edge` | Runtime adapters. |

## The Boundary

The core speaks `http` crate types plus a small set of its own. Hosts build
these from their native request type and render them back:

- `RequestCtx` — method, path, query, headers, body as reference-counted
  `Bytes`, WebSocket upgrade state, routing mode, request id.
- `ExecOutcome` — status, headers, a body that is either full bytes, a
  `ByteStream`, or a WebSocket duplex, and a disposition.
- `ByteStream` — the one stream type both directions use. Zero-copy
  passthrough is the default path; a stream is rewritten only when a
  transform has to.
- `CoreError` — classified as auth, routing, upstream, transport, or
  internal, with a framework-free render helper. Framework response
  conversions live in hosts, never in the core.

Host services enter through traits:

| Trait | What the host provides |
| --- | --- |
| `CredentialStore` | Mandatory. Loads secrets and persists rotated tokens atomically behind a version-guarded compare-and-swap. Claude rotates the refresh token on every refresh; an embedder without atomic persistence bricks its credential the first time the core refreshes it. |
| `CacheBackend` | TTL-aware shared cache for quota reservations, rate-limit counters, refresh leases, admission state, and login sessions. In-process for one instance; Redis, Upstash, or libSQL for several. |
| `UpstreamTransport` | Outbound HTTP and WebSocket. The trait lives in the core so the core never depends on a concrete client. |
| `UsageSink`, `CaptureSink` | The funnel's outputs: settlements and wire captures. The app writes rows; an embedder may aggregate or drop. |
| `Spawner` | Optional. Present: stream settlement detaches after the last byte (native servers). Absent: settlement completes inline before the stream closes (edge). The policy *is* the capability's presence; there is no policy enum and no `#[cfg]` fork. |
| `BindingStore` | Durable resource bindings (files, videos, tasks) scoped by provider and owner, so pooled follow-up requests reach the credential that created the resource. |
| `ControlPlane` | The narrow read view of providers, routes, keys, and settings the engine resolves against. The app implements it over its snapshot. |

Decryption is the store implementor's concern. `gproxy-app` does envelope
encryption inside its store; a bare embedder may store plaintext; the core
never sees a cipher.

## Execution Model

The engine is a fixed sequence of composable stages. Every path is a
composition of the same stages, and the last three are not optional:

```text
ingress → classify → authenticate → resolve(route | named target)
        → admit(permissions · rate limits · quota pre-charge)
        → transform? → request rules → channel.prepare → transport
        → response rules → reverse transform? → disposition
        ↺ failover within budget around prepare / transport / disposition
        → settle → capture → telemetry            ← the funnel: always runs
```

Two public tiers wrap this:

- **Tier 1, `invoke`** — one credential, one request in a known wire shape,
  no routing. Classify, prepare, send, settle. This is the embedder's SDK
  with pooled-credential discipline.
- **Tier 2, `execute`** — the full sequence: multi-credential failover,
  transforms, affinity. The gateway's data plane uses this and nothing else.

The funnel is enforced by type. Every `ExecOutcome` carries a `Settled`
proof that only the funnel module can construct, so no code path can return
a response that skipped settlement. A path that skips the funnel is not a
fast path; it is unmetered traffic, and it does not compile.

Service surfaces — the emulated vendor control planes behind Codex CLI,
Claude Code, and the other CLI channels — are declared, not coded ad hoc. A
channel registers a table of route patterns, each mapped to local synthesis
or an upstream forward. Local synthesis exits through the funnel with zero
usage; forwards exit through tier 1. WebSocket ingress works the same way: a
registry of declared upgrades, never an if-chain in the gateway.

## Request Lifecycle

```text
client request
  └─ host adapter: native request → RequestCtx
       ├─ /admin/api/**  → gproxy-admin dispatch (own session auth, audit log;
       │                    control plane, never touches usage)
       └─ data plane
            ├─ authenticate: API key → identity
            ├─ service surface / WebSocket registry match?
            │     ├─ yes → surface admit → local synthesis or tier-1 forward
            │     └─ no  → tier 2
            │              classify (OperationSpec) → resolve route or named target
            │              → admit: permissions · rate limits · quota pre-charge
            │              → transform if inbound wire ≠ upstream wire
            │              → channel.prepare (URL · auth · shaping)
            │              → transport send (zero-copy ByteStream)
            │              → disposition ──failover──▶ next credential
            └─ FUNNEL (always): settle usage · price · reconcile quota → UsageSink
                                capture → CaptureSink · telemetry
                                streams and sockets settle on end / close
  └─ host renders ExecOutcome (or a logged error exit: refund pre-charge,
     capture, telemetry, no settlement)
```

1. The host reads its native request, builds `RequestCtx`, and calls the app.
2. Surface tables are checked first. A matched entry is itself a stage
   composition.
3. Otherwise tier 2: classify via `OperationSpec`, authenticate, resolve,
   admit, transform if the inbound and upstream shapes differ, apply request
   rules, prepare in the channel, send, apply response rules, reverse
   transform, dispose, and fail over within the attempt budget.
4. The funnel settles usage, prices it, reconciles the quota reservation,
   writes the usage row and the wire capture, and emits telemetry. Streams
   settle on stream end.
5. The host renders `ExecOutcome`.

## The Operation Registry

An operation is declared once. `OperationSpec` in `gproxy-protocol` states,
per operation: its group, ingress path patterns per wire family, request
target, body and stream expectations, billability and settle mode, affinity
kind, and whether it is a WebSocket upgrade. Classification, channels,
routing defaults, settlement, and the console's generated metadata all read
that one declaration. In v2 the same facts were scattered across more than
ten match sites and five parallel billable-operation lists, and the fifth
list was the one that got missed.

Protocol enums are exhaustive inside the workspace. Adding a variant produces
a compile-error checklist of every site to update; there are no
`_ => unreachable!()` arms that compile clean and panic at runtime.

## Wire Models and Transforms

`gproxy-protocol` is a typed schema library, not an internal detail. Every
operation gets complete request, response, and stream-event models, verified
against the upstream API documentation. Two rules keep that maintainable
under specification churn:

- **Unknown fields survive.** Every model struct carries a flattened `rest`
  map, so a field the proxy has never heard of reaches the client unchanged.
  A proxy that drops unmodeled fields is wrong.
- **Absence is meaning.** Optional wire fields stay optional through models
  and transforms and are omitted on serialization. A fabricated
  `usage: {0,0,0}` where the upstream sent none lies to the client and to
  settlement, which must fall back to estimation instead.

Transforms are **pairwise by design; there is no intermediate
representation**. Upstream specifications change faster than any pivot
format can track, and conversion fidelity is the product. OpenAI Chat
Completions, OpenAI Responses, Claude Messages, and Gemini GenerateContent
have all six direct pairs in both directions, buffered and streaming. A
transport or envelope variant of an existing format — a wire shape over
WebSocket, an SSE re-framing, the Code Assist envelope shared by Gemini CLI
and Antigravity — composes on top of the existing pair instead of paying for
a new family.

Streaming transforms are explicit state machines. Three framings exist: SSE,
WebSocket, and Gemini's incremental JSON array, which is the default on
Gemini paths without `?alt=sse`. Framing is part of the wire contract on
both sides: the caller receives the framing it asked for regardless of what
the upstream produced. Truncated or invalid streams propagate interruption;
no decoder invents a successful terminator.

Provider-native tools — Claude `bash` and `text_editor`, Responses `shell`
and `apply_patch`, Gemini `code_execution` — map to the nearest native tool
on the target, approximate rather than identical, with the call and result
lifecycle kept correlated across the conversion. Where no counterpart
exists, the tool degrades to a declared function tool rather than being
dropped.

## Channels

A channel is the upstream adapter. The `Channel` trait is synchronous and
object-safe: preparation, classification, stream decoding, and usage
extraction are pure logic over borrowed data, so the registry holds plain
trait objects. The one asynchronous concern, OAuth refresh, returns a boxed
future and persists through the host's version-guarded store under an
exclusive cache lease.

A channel declares, as data:

- its **capability rows** — which client-facing operation maps to which
  channel-native operation, so the registry knows exactly which transform
  pair a request needs;
- its **routing defaults** — passthrough, transform, local, or unsupported
  per operation and inbound protocol, seeded into every new provider and
  recomputable on reset;
- its **service-surface tables** and any **operation drivers** (multi-step
  browser turns such as Claude Web are declarative state machines whose
  side calls the core transports and funnels);
- its **client fingerprint** — ALPN, TLS version range, cipher and curve
  lists, HTTP/2 settings, header order — as plain data. The transport holds
  exactly one generic translation and knows no channel names;
- its **login modes** — authorization code with PKCE, device code, or cookie
  exchange — which the console renders without channel-specific UI;
- the **fields** its provider and credential forms need, with labels and
  help text, likewise rendered generically.

Twenty-eight channels ship: API-key providers, cloud platforms with
service-account or SigV4 auth, aggregators, and the CLI emulations (Codex,
Claude Code, Gemini CLI, Copilot CLI, Kiro, Cline, Grok Build, OpenCode,
WorkBuddy, Antigravity). Claude Web is native-only because its live tool
continuation is process-local.

## Routing and Admission

Model preprocessing runs alias → suffix → route. A route's members carry a
`tier` (the failover level: everything in tier 0 is tried before anything in
tier 1) and a `weight` (the split inside a tier). Credentials carry a weight
too. Selection is a deterministic counter rotation over the weight-expanded
sequence — no randomness, so wasm and native behave identically and a replay
is reproducible. Health is tracked per credential and model, so a 429 on one
model does not poison the credential's other models.

Everything that gates or meters traffic goes through `CacheBackend`: quota
reservations, rate-limit counters, admission state, refresh leases. With the
in-process backend an instance is a single instance, which is a supported
default. A shared backend is what makes a second instance correct.

## Rules

Operators configure two rule layers per provider without a code change.
**Routing rules** decide per operation and inbound protocol whether a request
passes through, transforms to a target, is answered locally, or is
unsupported. **Rule sets** hold ordered mutations — system text, cache
breakpoints, JSON rewrites, structural transforms, headers — applied to the
provider-native request after the transform and before the channel, and to
responses before the reverse transform. Streaming rules rewrite each
completed frame as it passes; nothing buffers a live stream to completion.
Channel shaping compiled into the binary is vendor behaviour; rules are what
the operator changes at runtime.

## Billing and Settlement

There is one `Settlement` construction site. `NormalizedUsage` keeps
first-class token fields plus `metrics` and `dimensions` maps. New usage
measures — audio seconds, images, search calls, cached tokens — go into the
maps and are priced by data-driven rate rules; that path costs two touch
points. Promoting a metric to a column happens only with evidence.

Pricing has two axes on the same rows. A tier row may name a `service_tier`
(batch, priority, flex), a `min_prompt_tokens` threshold (long-context
ladders), or both, carrying either explicit prices or a multiplier. An
explicit tier price replaces the base ladder; a multiplier composes with it.
The request declares a tier at admission for the pre-charge estimate; the
response reports the tier actually served, and settlement charges that one.

Realtime calls are metered from the server side: the proxy opens OpenAI's
sideband connection for the returned call id and reads usage from
`response.done` and transcription-completed events. Client-reported totals
are never trusted.

## Persistence

One schema catalog, one query-building layer. SeaQuery emits the DDL and
every business statement for SQLite, PostgreSQL, and MySQL; the SQLite
dialect also runs over libSQL's Hrana protocol, which is how the wasm host
persists. Backend parity is a test that compares table and column sets and
runs one shared scenario across every backend, not a discipline. Migrations
are numbered and monotonic, and a database migrated from the first version
converges with a fresh one.

Control-plane reads go through a snapshot rebuilt on writes and swapped
atomically. A write also bumps an invalidation counter in the shared cache;
native instances poll it once per second and edge isolates check it on each
request, then rebuild their snapshot when the version changes.

## Admin Plane and Web Surfaces

`gproxy-admin` holds the DTOs and a pure dispatch function. Every host calls
the same dispatch for `/admin/api/**` and `/portal/api/**`; there is no
framework-specific admin router. Admin DTOs derive their TypeScript
definitions, regenerated as part of `cargo test`, and the console imports
those generated files rather than declaring its own — a hand-written mirror
of a Rust type is a bug even when it happens to match.

The same binary serves three surfaces from one React application: the public
product page at `/`, the operator console at `/admin`, and the user portal at
`/portal`. See [Console, Portal & Public Site](/guides/console/).

## What Each Host Adds

- **Native (`gproxy`)** — the axum listener with ingress size and concurrency
  limits, request ids, graceful shutdown; a `Spawner`, so stream settlement
  detaches; the `wreq` transport with per-channel TLS fingerprints and proxy
  selection; the signed announcement feed, per-user autostart, and
  self-update with a signed manifest and rollback. Configuration is
  environment plus `.env`, read once at start.
- **Edge** — a typed config assembled from platform bindings, Fetch requests
  converted to `RequestCtx`, streams with pull and cancellation draining so
  inline settlement completes, WebSocket pumping where the platform offers an
  upgrade and an explicit 501 where it does not. libSQL is the store; Upstash
  is the optional shared cache.
- **Embedding** — `App::start(config)` returns a handle with execute, mutate,
  reload, and shutdown; or link `gproxy-core` alone, implement the host
  traits, and call `invoke` or `execute` directly. See
  [Embedding the Core](/reference/embedding/).

## Rules That Keep It This Way

- The core never depends on a server framework or a UI.
- Every request that reaches an upstream exits through the same funnel.
- An operation is declared once, in `OperationSpec`.
- Protocol enums are exhaustive; the compiler produces the checklist.
- Transforms are pairwise; there is no intermediate representation.
- One schema definition, one query layer, parity enforced by test.
- New usage measures are dimensions first, columns only with evidence.
- wasm is a first-class core target; runtime differences are host
  capabilities, not `#[cfg]` forks.
- Bodies move as reference-counted bytes and are parsed once, at the stage
  that needs them.
- Frontend types are generated from Rust and never edited by hand.
