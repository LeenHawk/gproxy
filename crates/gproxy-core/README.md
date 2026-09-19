# gproxy-core

English | [简体中文](README.zh-CN.md)

The provider execution engine, without downstream concerns. The upper layer
resolves routes and model aliases, authenticates callers, evaluates policies and
admits requests; it then hands core a Store, a Cache, an Observer and one
`ExecutionTarget` (provider, upstream model, permitted credentials). Core does
protocol conversion, request/response rewriting, credential selection, refresh
and availability, upstream calls over HTTP and WebSocket, stream forwarding,
observation and settlement, continuation state and resources for multi-call
adaptations, agent session assignments, account quota observation, usage
extraction and local token estimation.

| Module | Role |
|---|---|
| `builder` | `Core::builder(store)`: cache, observer and secret codec required; channels registered into a `ChannelRegistry`; optional client pool and file storage |
| `data` / `runtime` / `context` | Execution snapshot, atomic credential material, blocks and window keys, resolved target and request/attempt/exchange contexts, usage reports |
| `secret` | `SecretCodec`: `AesGcmCodec` (AES-256-GCM envelope, per-credential data key, credential ID in the AAD) by default, `PlaintextCodec` as the explicit opt-out |
| `limits` | `ExecutionLimits` from the Setting row, deriving every `CapabilityLimits` and `CodecLimits`; no unlimited mode |
| `assemble` | ControlData rows into a `CoreData` snapshot: profiles to pooled clients, channel lookup, secret opening, endpoint validation, rule compilation, `QuotaModel` dimensions, custom vocabularies, live blocks |
| `rewrite` | Rule compilation, selection by phase/operation/model/headers, Body/Header/Query application, raw-preserving per-unit stream rewriting (SSE, JSON array, NDJSON) |
| `select` / `availability` | Credential choice inside the permitted set with strategy, affinity and blocks; failure streaks and cooldowns |
| `execute` (private) | HTTP and WebSocket attempt loops, observed client/body/socket wrappers, request buffering, the settlement funnel and its `Settled` proof |
| `convert` | Passthrough-or-convert routing, native endpoints, and one driver per protocol family running protocol's adaptation flows over the attempt-bound upstream |
| `capability` | Core's `AttemptUpstream`, `ProtocolState` and `Resources`, the host capabilities protocol adaptations are generic over |
| `refresh` | Explicit credential refresh: cross-instance lease, channel `CredentialRefresh`, seal, version CAS, publication, `Dead` on definitive rejection |
| `quota` | `QuotaHeaders`/`QuotaQuery` observations into `credential_quota_cycles` and exhaustion blocks; Counted dimensions metered in the cache |
| `session` | Agent session assignments: stay bound while usable, reserve a new generation on durable failure, activate or fail from the preparing attempt |
| `estimate` | Local token estimation for exchanges the upstream did not meter |
| `observe` | The host's settlement/capture/trace funnel with a pre-work policy query |
| `keys` | The one cache key grammar and the invalidation topic |

## Snapshot and data

`CoreData` holds providers, credentials, reusable rewrite sets, the execution
limits and the estimator. It does not hold routes, exposed-model aliases,
identity tables, permissions, OAuth client allowlists, subscriptions, pricing or
admission rules; those belong to the upper layer, together with route affinity
and cross-provider balancing. `publish_snapshot` accepts only a newer revision;
requests pin their own `Arc`, so a reload never changes a request midway.

`ExecutionTarget` names one provider, an optional resolved upstream model and
the exact permitted credential set. Core rotates and retries only inside that
set; an empty set is "no usable credential", never the whole provider pool. The
upper layer supplies the attempt budget, the deadline and an opaque isolation
scope. Core does not interpret user or key roles. Bound remote resources travel
with their original target and restricted credentials.

`CredentialData` carries the resolved HTTP client, the same profile forced to
HTTP/1.1 for WebSocket upgrades, the `QuotaDimension`s the channel declared for
this credential, and a shared `CredentialState`: secret, expiry, lifecycle status
and Store version publish atomically as one `CredentialVersion`, each attempt
pins one, and only a newer version replaces it. `Dead` credentials are never
selected or refreshed; `status_reason` says why, and `CoreError::CredentialDead`
tells the caller a person must log in again.

Availability is one `CredentialBlocks` cache payload per credential, rebuilt
from `credential_blocks` rows on load. Each block names a channel `QuotaScope`,
an optional operation, an `until_ms` and a `BlockSource`: a Reported dimension
observed exhausted (with its persisted cycle), a Counted window used up, upstream
rate limiting, or consecutive failures. Streaks are per scope and operation, so a
broken model neither hides behind nor poisons a healthy one. See
[credential availability](../../design/core-credential-availability.md).

`CredentialStrategy` is `RoundRobin`, `Sticky` or `RoundRobinAffinity`
(`round_robin_affinity`). Sticky and affinity honour a session pin written after
a successful attempt; unbound requests advance a shared rotation counter keyed by
the eligible set. Pins are scoped by caller scope and provider.

## Public API

[api/operations.rs](src/api/operations.rs) declares 30 named HTTP methods and 3
named WebSocket methods matching `BaseChannel`, plus the generic `send` and
`connect`. Named methods reject a mismatched context operation. Every method
returns an `Execution<T>`: the protocol response or connection and a
`UsageCompletion` that resolves when the request settles.

[api/lifecycle.rs](src/api/lifecycle.rs) is the Store/account surface:

| Method | Behaviour |
|---|---|
| `load_data` / `reload_data` | One `load_control_data` batch, assembly, live blocks warmed into the cache, custom vocabularies parsed once from file storage; publish monotonically, a failed load keeps the previous snapshot |
| `reload_credentials` | Re-read rows in input order and publish into the existing slot; missing rows return None and retire the slot |
| `refresh_credential` | Provider ownership check, per-credential cache lease (peers wait, a newer durable version satisfies the call), authoritative Store read, channel refresh, seal, `refresh_many` CAS, publication and a `CredentialChanged` notification; `RefreshRejected` persists `Dead` with its reason |
| `query_credential_quota` | The channel's `QuotaQuery` through the assigned client; every entry becomes a `credential_quota_cycles` row, exhausted declared dimensions become blocks |

## Execution

Each attempt selects a credential inside the permitted set (enabled, Active,
not retired, not blocked for the model and operation), refreshes material that
expires within a minute, charges Counted request dimensions before anything goes
out, prepares the request and dispatches through `ChannelBinding` with the
provider's method URL. A streaming request body is buffered up to the limit so
retries can replay it; over the limit the request becomes single-attempt.

Answers are classified: 2xx and client 4xx are returned; 429 writes a
`RateLimited` block (Retry-After or 30s) unless the channel's `QuotaHeaders`
reported the exhausted dimension, which writes a `QuotaExhausted` block until
the upstream reset instead; 401/403 on a refreshable credential forces one
refresh and retries the same credential; 5xx and transport errors bump the
per-scope failure streak and retry, writing a cooldown block at three. When
the budget or the candidates run out, the last upstream answer is returned.
Every answer's headers are handed to `QuotaHeaders`, so account limits are
observed on success too.

Every response body handed back is one observed stream: capture, usage
observation, per-unit response rewriting, read cap, idle timeout and
cancellation; ending or dropping it settles the request exactly once, and usage
is recorded before anything else can settle. `Execution` can only be built with
the funnel's `Settled` proof. WebSocket operations run the same loop for the
handshake; an established socket is captured, metered and rewritten in both
directions and settles when it ends or is dropped.

## Conversion

`convert::route` asks the channel which dialects the provider speaks natively
for the operation (an `OperationRule` with `action = "dialects"` overrides it):
a native client dialect passes through, otherwise the first declared dialect is
the conversion target. A streaming client against an upstream that only
generates buffered is served by `Route::Synthesize`: one buffered upstream call,
then the client's native stream lifecycle replayed from the result.

Conversion runs protocol's adaptation flows over `AttemptUpstream`, so every
native call inside a flow is captured, metered and rewritten like a passthrough
call, and a rejected native answer re-enters the same classification.

| Family | Coverage |
|---|---|
| Generate | All twelve dialect pairs, buffered and streamed; Chat `n` / Gemini `candidateCount` fan out into journaled child calls; Chat, Claude and Gemini clients over a Responses WebSocket upstream; Responses WebSocket clients served turn by turn over Chat, Claude or Gemini HTTP upstreams |
| Models | List and get across OpenAI, Claude and Gemini using provider model supplements |
| Count tokens | Claude and Gemini targets; OpenAI as a target is deliberately unsupported |
| Embeddings | OpenAI ↔ Gemini, single and batch |
| Guardian, compact, memory | Any of the four targets; guardian streams are refused |
| Files | Retrieve, list, delete, content and multipart upload including Gemini's resumable protocol |
| Images | OpenAI create/edit over Gemini or the Responses image tool; URL delivery refused |
| Video | OpenAI native video over Veo with job state in `ProtocolState` |

## Credential lifecycle, quota and sessions

`refresh_credential` is an explicit account operation; the attempt loop calls
it `IfNeeded` before pinning expiring material and `Force` after a 401/403.
Quota has two tracks: Reported dimensions receive values from headers, queries
or exhaustion replies and block until the upstream period end (or one window
derived from the dimension); Counted dimensions are metered in the cache under
`CountedWindowKey`, requests before the exchange and tokens after usage settles,
and block until the window ends. Entries that match no declared dimension are
still persisted as cycles.

A request whose `SessionIdentity` names an `agent_sessions` row is an agent
session: core keeps it on its active assignment's credential while usable, runs
unbound through another credential for transient trouble (rate limit, cooldown,
this request's 5xx), and reserves a new generation only when the credential is
durably unusable (dead, disabled, retired, quota or counted block), carrying the
exhausted cycle and the triggering request as evidence. The preparing attempt
activates the reservation on any upstream answer and fails it otherwise;
concurrent first requests converge on the pending reservation.

## Usage and estimation

`NormalizedUsage` is reused from gproxy-channel: one request can have several
attempts, each with several physical exchanges, reported once per capture ID.
An exchange the upstream served without reported usage (or with missing token
counts) is estimated locally when the Setting row enables usage: input tokens
from the request's prompt text counted by the tokenizer for the upstream model
(catalog vocabulary file, Setting default, or bundled encoders), output tokens as
half the response bytes. Estimates are `Partial` and carry
`dimensions["estimated"] = "true"`; reported values are never overwritten and
rejected answers are not estimated. `UsageState::Skipped` means the request's
policy disabled usage, distinct from an upstream that reported nothing.

## Observation

[observe.rs](src/observe.rs) is the host's funnel. `Observer::policy` answers
once per request before any attempt; disabled work is never performed and
discarded. `capture` opens one `CaptureSink` per physical exchange; `usage` is
called exactly once per request with usage enabled, including cancelled and
failed ones; `trace` receives borrowed attempt, exchange and refresh events.
Recording never rewrites, reorders or delays the delivered stream.

## Protocol capabilities

| Type | Trait | Binding |
|---|---|---|
| `AttemptUpstream` | `Upstream<Target = OperationKey>` | One attempt's provider, pinned credential version and client; owned and cloneable so lazily started fanout children outlive the attempt frame |
| `ProtocolState` | `StateStore<Scope = StateScope>` | Store ProtocolState rows; scope is caller scope, provider and optional conversation |
| `Resources` | `ResourceAccess<Scope = ResourceScope>` | Publications in `resource_bindings` and `file_objects` through file storage; `Id` reads resolve a same-scope publication first, then the scope provider's files API; `Url` is unsupported |

## wasm32

The data contracts compile on wasm32; `load_data`, `send`, `connect`,
`refresh_credential` and `query_credential_quota` return
`CoreError::NotImplemented` there because there is no outbound transport yet.

```sh
cargo test -p gproxy-core
cargo clippy -p gproxy-core --all-targets -- -D warnings
cargo clippy -p gproxy-core --target wasm32-unknown-unknown --all-targets -- -D warnings
```

Tests run against an in-memory SQLite Store, the memory cache and scripted
channels/clients; they establish the engine's contracts, not a working gateway
or a real provider. See [crate boundaries](../../design/crates.md).
