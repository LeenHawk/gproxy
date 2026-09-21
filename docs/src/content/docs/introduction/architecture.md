---
title: Architecture
description: How GPROXY v4 is built — an engine with no server, a product layer with no transport, three hosts over one router, and the seams that keep them apart.
---

v4 is a ground-up rewrite of v3. The job is the same; the structure is not.
This page is the map: the crates, the seams between them, and the rules that
keep the structure from drifting back.

The one sentence that explains the rest: **each layer decides exactly one class
of question, and hands down a narrowed answer that nothing below re-opens.**

## The Two Assemblies

```text
sdk  =  engine + configuration writes + login + resolution + queries + sync
app  =  sdk + identity and admission + the typed admin / portal / issuer operations
host =  app + a transport
```

`gproxy-sdk` is the embeddable handle. It assembles the engine out of default
implementations, owns the configuration writes that advance
`settings.config_revision`, turns a login into a credential row, resolves a
model name to an execution plan, and keeps every instance of a deployment on
the same revision. It contains **no HTTP server and no identity**.

`gproxy-app` adds who is calling and what they may reach: users, gateway API
keys, organizations, teams, permissions, subscriptions, rate limits, the OAuth
issuer, audit, and the operations that write any of them. It owns **no server**
— no router, no runtime, no CLI — which is what lets the same decisions run
behind a native axum server and inside a Cloudflare Worker.

A host is the thin remainder: bytes in, typed calls out, bytes back.

## The Crates

| Crate | Holds | Never |
| --- | --- | --- |
| `gproxy-protocol` | Connection modelling (`WireRequest`, `WireResponse`, bodies, framing, sockets), the operation registry, the OpenAI / Claude / Gemini wire types, and the host-capability interfaces protocol adaptation needs | Knows a channel exists; matches a URL path |
| `gproxy-channel` | `BaseChannel`, the optional ability traits, and the 25 upstream adapters behind Cargo features | Depends on the engine or the store; picks a transport backend |
| `gproxy-client` | The outbound transport contract and its `reqwest` / `wreq` implementations, plus the wasm `fetch` and Workers backends | Reads the database, picks a credential, converts a protocol |
| `gproxy-core` | The engine: the provider execution snapshot, credential selection, refresh and health inside a permitted set, conversion, rewriting, budgets, pricing, settlement and observation | Resolves a route, checks a permission, runs a server |
| `gproxy-store` | The schema and its queries, backend by feature | — |
| `gproxy-cache` | TTL state, atomic counters, permits and leases, cross-instance invalidation; Memory and Redis | Business entities; a silent local fallback when the backend fails |
| `gproxy-seaorm` | SeaORM batch reads and writes, the D1 binding, type mapping, schema sync | Business entities |
| `gproxy-tokenizer` | String to token count, against a local shared vocabulary | Parsing requests, downloading, background work |
| `gproxy-file` | Optional local or S3/R2 content storage over OpenDAL | File metadata; ownership |
| `gproxy-sdk` | The handle: assembly, `manage()`, `login()`, resolution, sessions, `call()`, `query()`, `sync` | Any HTTP server code; any identity |
| `gproxy-app` | Identity, admission, the two product surfaces and the issuer | A transport |
| `gproxy-host-axum` | HTTP path → operation. The route table | A product decision of its own |
| `gproxy-host-edge` | `fetch` → the axum router | A second route table |
| `gproxy-host-tauri` | IPC → operation, for the management and user surfaces only | The data plane |
| `gproxy` | The command line: the environment, a file, a socket, a terminal | Anything a library could have done |

Arrows point one way. A host depends on the app, the app on the sdk, the sdk on
the engine. The engine never gains a listener, a router or a UI.

## Where Each Question Is Answered

| Question | Answered by |
| --- | --- |
| who is calling | `gproxy-app`'s authenticator |
| what they may reach, and whose money it is | `gproxy-app`'s admission |
| which provider serves this model name | the sdk's resolver |
| which credential, and what to do when it fails | the engine |
| what an operation *does* | `gproxy-app`'s operation families and the sdk's `manage()` |
| what a failure is worth on the wire | `AppError::status_code` / `code` |
| which URL means which operation | the axum host |

A host that re-answered any of the first five would apply its answer over HTTP
and not over IPC, which is exactly the class of bug the seam exists to prevent.

## Resolution and the Plan

Resolution happens **before** the engine, so the engine only ever sees an
already-resolved target. A model name matches the first rule that applies:

| Name | Resolves to | Attempt budget |
| --- | --- | --- |
| absent | every enabled provider, no upstream model | `settings.max_attempts` |
| an exposed model name | that route's enabled members | the route's own |
| `channel/model` | the providers of that channel, preferring those whose catalogue lists `model` | `settings.max_attempts` |
| `provider/model` | that one provider | `settings.max_attempts` |
| anything else | an unknown-model error | — |

Exposed names are matched exactly and **before** the prefix forms, so an
operator can expose the literal name `openai/gpt-5`. Inside the prefix forms a
channel id beats a provider of the same name: a channel id is fixed by the
build and cannot be renamed out of the way, while a provider always can.

Candidates are then narrowed by channel, by the allowed provider ids and by the
allowed credential ids — the last is how the product layer keeps one
organization's credentials out of another's requests. Order is
`(tier, health, descending weight, stable id)`; only the leading run is
balanced, by the route's strategy.

## Failover Has Two Axes

The engine already retries within one provider's credentials. The sdk is the
other axis and moves to the next provider **only for failures another provider
could serve**: no usable credential, a dead credential, a continuation pinned
elsewhere, any channel or transport error, and a 401, 403, 429 or 5xx answer. A
spent budget, a forbidden request, a cancellation, a bad transform and a store
or cache failure all stop the call, because retrying them elsewhere only costs
more.

The attempt budget is shared across targets, so a plan cannot cost more
upstream calls than its `max_attempts`. With no target left the last answer is
returned as it stands — a 429 from the final provider is the caller's 429, not
a synthesized error.

## One Write, One Revision, One Notification

```text
commit_revision([statements…, config_revision += 1, read back])   one transaction
      ↓
reload   the snapshot this instance serves
      ↓
publish  ConfigurationChanged { revision, scopes }
```

The rows and the revision bump are the same transaction. Split them and two
real windows open: rows land without a bump, so peers never learn to reload and
the change is invisible on every other instance; or the bump lands without the
rows, so every peer reloads for nothing.

The reload happens **before** the notification. The reverse would announce a
revision this instance cannot yet serve. The notification itself is best
effort — a cache that refuses it costs the deployment one poll interval, which
is why the poll exists.

Two mechanisms, because neither alone is enough:

| | Carries | Fails by |
| --- | --- | --- |
| an invalidation on the shared cache | "look again", within milliseconds | losing messages |
| the `settings.config_revision` poll | the durable truth, every 30 s | being slow |

Reloads are serialized and monotonic; an older revision never replaces a newer
one, and a failed reload leaves the previous snapshot serving.

## Settlement Is Not Optional

The engine guarantees that every path which reaches an upstream passes the same
funnel: price the exchange, charge every applicable budget window, hand the
report to the observer. There is no fast path around it, because a path around
it is unmetered traffic.

What *is* optional is asked **before** the work, not discarded after it. A
capture that is switched off allocates nothing and clones no body; v3's
arrangement paid the whole cost before a sink could decline it.

Cost is known only when the exchange ends, so a budget can be overrun by at
most one request. That is the accepted price of not estimating, not
pre-charging and not rolling back. Settlement is idempotent per request id, and
a settlement failure loses the accounting, never the delivered response.

A model with no price rule settles at zero with the dimension
`unpriced = true`. The operator wanted the signal, not a refusal.

## wasm Is a Target, Not a Fork

`gproxy-app` builds for `wasm32-unknown-unknown` and so does the axum router,
which is why `gproxy-host-edge` mounts the same `Router` instead of writing the
route table twice. axum's server halves are Cargo features rather than the
crate; what the wasm build leaves out is socket-shaped and never a route.

The real obstacle was `Send`. On wasm the engine is `!Send` **by design** — a JS
transport handle belongs to the isolate that made it — while axum wants
`Send + Sync` state and `Send` handler futures. Two runtime-checked bridges fix
it, both sound because a Worker isolate is single-threaded: the app holds its
handle in a `SendWrapper`, and the axum host wraps each handler body. A handler
that forgets the wrapper is a **compile error on the wasm target naming that
handler** — which is the enforcement, and the reason it sits at the handler
rather than behind a `cfg` on the router. A route that cannot compile for the
edge cannot silently fail to exist there.

## Rules That Keep It This Way

- The engine never depends on a server framework, a router or a UI.
- Every request that reaches an upstream exits through the same funnel.
- Conversion is pairwise; there is no intermediate representation, and a
  converter never reads or writes the unknown-field bag.
- A configuration write is one transaction with its revision bump.
- A snapshot publication is monotonic; a reload never goes backwards.
- Assembly never fails on one bad row — it is dropped, counted and logged.
- The edge host holds no route table of its own.
- Frontend types are generated from Rust and never written by hand.
