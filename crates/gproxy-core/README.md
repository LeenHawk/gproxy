# gproxy-core

English | [简体中文](README.zh-CN.md)

Provider execution data contracts. The upper layer resolves routes/model aliases,
authenticates callers, evaluates policies and admits requests before calling core.
This crate currently supplies data structures and monotonic in-memory publication;
request execution and credential selection are not wired yet.

| Module | Structures |
|---|---|
| `data` | Provider/credential execution snapshot, channel/client handles, prepared rewrite matchers |
| `runtime` | Atomic credential material, credential affinity/rotation, scoped availability blocks, counted-dimension window keys and execution-data invalidation |
| `context` | Resolved execution target, opaque caller scope/session, request/attempt/exchange, usage reports |
| `observe` | Host-implemented settlement/capture/trace funnel with a pre-work policy query |
| `capability` | Core implementations of protocol's Upstream, StateStore and ResourceAccess, bound to an attempt or scope |
| `assemble` | ControlData rows into a CoreData snapshot: profiles to clients, channel lookup, secret opening, rule compilation, quota dimensions, live blocks |
| `rewrite` | Rule compilation (regex, dot paths, filters) with the target/phase/name rules from the rewrite design |
| `keys` | The one cache key grammar every core reader and writer shares |

`CoreData` contains providers, credentials and reusable rewrite sets. It does not
contain routes, exposed-model aliases, identity tables, permissions, OAuth client
allowlists, subscriptions, pricing or admission rules. These belong to the upper
layer, along with route affinity and cross-provider balancing/failover.

`ExecutionTarget` supplies one selected Provider, an optional resolved upstream
model and the exact permitted credential set. Future execution must rotate/retry
only within that set; an empty set must never fall back to the global provider
pool. The upper layer supplies the final attempt budget and an authenticated,
opaque isolation scope. Core does not interpret user/key roles. Bound remote
resources must be passed with their original target and restricted credentials;
assignment references are attribution, not permission to move a resource.

`CredentialStrategy::RoundRobinAffinity` serializes as `round_robin_affinity`.
New/unbound sessions receive credentials in round-robin order; subsequent requests
reuse the same eligible credential without advancing the rotation cursor. A missing,
expired or unusable/disallowed pin triggers reassignment within the supplied set.
Without a stable session identity, use ordinary per-request round-robin. Scope pins
by the caller isolation scope and Provider; do not share them across callers.
Concurrent first requests for the same session must converge on one binding. This
is a selection contract only: persistence and the selector are still pending.

`Core<C>` is assembled through `Core::builder(store)`: `cache`, `observer` and
`secret_codec` are required, `channel`/`channels` register `BaseChannel` implementations
into a `ChannelRegistry` (duplicate IDs rejected), `client_pool` overrides the owned
`gproxy_client::ClientPool`. Construction does no I/O and nothing defaults to a no-op:
a host that wants no settlement or unencrypted secrets must say so explicitly.
`SecretCodec` seals credential secrets at rest; `AesGcmCodec` (AES-256-GCM envelope,
per-credential data key wrapped by the master key, credential ID in the AAD) is the
default choice and `PlaintextCodec` the explicit opt-out; each refuses the other's
envelopes. `ExecutionLimits` come from the Setting row (request/stream-idle timeouts,
body/event/frame byte caps, multipart parts) and derive every `CapabilityLimits` and
`CodecLimits`; there is no unlimited mode, and a request deadline can only shorten them. `publish_snapshot` accepts only
a newer revision of an already-validated execution snapshot. Requests pin their own Arc.
Core load/reload prototypes own execution-data assembly from Store; durable revision
allocation and notification coordination remain upper-layer integration.

`CredentialData` holds execution configuration, the resolved HTTP client, the same
profile forced to HTTP/1.1 for WebSocket upgrades, and a shared `CredentialState`.
Secret, expiry, lifecycle status and Store version publish atomically as one
`CredentialVersion`; each attempt pins one version. Publish only after Store CAS or authoritative loading.
Equal/older versions are rejected. Reuse a slot for a still-live credential, retire
it on deletion, and create a new slot for a recreated credential. There is no
implicit refresh lease or persistence in the publication method.

Credential availability is one `CredentialBlocks` cache payload per credential, not a
quota ledger; each block is also a Store `credential_blocks` row, so the partially
limited state survives restarts and the cache is rebuilt from Store on load. Each `CredentialBlock` names a channel `QuotaScope` (all, model list or
family prefix), an optional operation, an `until_ms` and a `BlockSource`: a Reported
dimension observed exhausted (tied to a persisted `CredentialQuotaCycle`), a Counted
dimension's window used up, rate limiting, or consecutive failures. Failure streaks are
kept per scope/operation as `FailureStreak`, so a broken model neither hides behind nor
poisons a healthy one. Selection asks
`blocked_by(model, operation, now)`; a request without a model is only affected by
credential-wide blocks, and an `Unknown` scope blocks the whole credential. Core never
expands a family prefix into a model list. Durable lifecycle is separate:
`CredentialData.status` mirrors the Store column, `Dead` is never selected or refreshed,
and `status_reason` says why. Refresh writes `Dead` on a definitive rejection;
`CoreError::CredentialDead` tells the caller a person must log in again.
`CredentialData.quota` holds the
`QuotaDimension`s the channel's `QuotaModel` declared for this credential at assembly;
Counted dimensions are metered in the cache under `CountedWindowKey`. Remaining
capacity itself stays in the channel's `QuotaSnapshot` and Store cycles; see
[credential availability](../../design/core-credential-availability.md).

CoreData and decrypted material are not Debug/Serialize. Session identity is
provided by ingress; RequestFallback is not cross-request stable. Protocol request,
response and stream types remain in gproxy-protocol. `NormalizedUsage` is reused
from gproxy-channel: one request can have multiple attempts, each with multiple
physical exchanges. Capture IDs belong to physical exchanges, not stream chunks.
`UsageReport` reaches the host's Observer funnel and, as the same value, the caller's
`UsageCompletion`; it contains no charges, subscription allocations or admission
reservations. `UsageState::Skipped` means the request's policy disabled usage, which
is distinct from an upstream that reported nothing.

`RewriteTarget` represents Body (owning optional JSON paths), Header (a typed
HeaderName), or Query (a decoded parameter name). Header/Query operate on repeated
values without flattening them; Query is request-only. Compiler validation and
actual mutations are pending; see [rewrite design](../../design/core-rewrite.md).

ProviderData also carries `operation_urls`, keyed by the destination
`OperationKey` and HTTP/WS transport. `operation_url` looks up the enabled,
validated override prepared from Store OperationEndpoint rows; None means use
provider/channel defaults. Applying the URL during channel execution remains
pending with the other execution prototypes.

## Public API

The operation facade is declared in [api/operations.rs](src/api/operations.rs):
30 named HTTP methods and 3 named WS methods match BaseChannel, with generic
`send`/`connect` dispatchers. Named methods reject a mismatched context operation;
they never silently replace an operation already authorized by the upper layer.
The HTTP match is exhaustive, so adding a protocol operation requires review of
its core entry point. Transport payloads retain the existing protocol types.

| Group | Named entry points |
|---|---|
| Models/counting | `list_models`, `get_model`, `count_tokens` |
| Generation | `generate_content`, `stream_generate_content` |
| Review/context | `guardian_review`, `guardian_classify`, `compact_content`, `summarize_memory`, `create_conversation` |
| Embedding/retrieval | `create_embedding`, `batch_create_embedding`, `rerank`, `web_search` |
| Images/audio | `create_image`, `edit_image`, `create_speech`, `create_transcription`, `create_translation` |
| Files | `create_file`, `list_files`, `retrieve_file`, `retrieve_file_content`, `delete_file` |
| Video | `create_video`, `retrieve_video`, `list_videos`, `delete_video`, `download_video_content` |
| Realtime/WS | `create_realtime_call` (HTTP), `connect_realtime`, `generate_content_websocket`, `stream_generate_content_websocket` |

[api/lifecycle.rs](src/api/lifecycle.rs) declares the Store/account surface:

| Method | Contract |
|---|---|
| `load_data` | Implemented: one `load_control_data` batch → `assemble` (enabled providers to registered channels, credential → provider → setting profile resolved to a pooled client, secrets opened, endpoints validated, rules compiled, `QuotaModel` dimensions declared, `config_revision` and limits read) → live `credential_blocks` warmed into the cache; any invalid row fails the whole load |
| `reload_data` | Implemented: load then publish monotonically; a failed load keeps the previous snapshot active |
| `reload_credentials` | Implemented: rows re-read in input order, material published into the existing `CredentialState` slot, missing rows return None and retire the slot; a row created after the last reload is reported but needs `reload_data` |
| `refresh_credential` | Explicit provider/credential IDs plus IfNeeded/Force; lease, current Store read, channel refresh, seal/CAS persistence, publication |
| `query_credential_quota` | Upstream account observations via the assigned channel/client; no subscription aggregation or quota reset |

Credential methods return secret-free `CredentialSummary` (version, expiry, durable
`CredentialStatus`) or existing channel
`QuotaSnapshot`, not decrypted material. The trusted upper layer authorizes IDs.
`load_data` does not load route, identity, permission, subscription or price tables;
it neither opens the database nor calls schema sync. Channel/client registration,
secret codecs and durable revision integration remain prerequisites for assembly.

Actual HTTP/WS execution, credential refresh and quota queries still return
`CoreError::NotImplemented`; on wasm32 `load_data` does too, because there is no
outbound transport there yet. No fake upstream calls are performed. `Execution<T>` holds the
protocol response and eventual `UsageCompletion`; stream lifecycle implementation
remains pending.

Removed public placeholders: `select_credential`, `compile_rewrite_rule`,
`rewrite_request`, `rewrite_response`, `record_outcome`. Internal selection,
attempt preparation, rewriting, health/affinity updates, continuation/resource
persistence and capture writes will be added with their implementations, not as
public phase hooks or duplicate CRUD. `AttemptOutcome` survives only as trace data
delivered to the Observer. Cancellation already uses RequestContext's token/deadline;
there is no unused shutdown/background-worker API. Routing, policy, OAuth login,
billing and management CRUD stay outside core.

## Observation

[observe.rs](src/observe.rs) is the extension point behind the settlement, capture
and telemetry switches from [crate boundaries](../../design/crates.md). Core
guarantees every execution path reaches it; the host persists, redacts, prices
and retains. The contract is ask-first: `Observer::policy` answers once per
request, before any attempt, and disabled work is never performed and discarded.

| Item | Contract |
|---|---|
| `ObservationPolicy` | `usage`, `capture` (`Off`/`Metadata`/`Full`) and `trace`, decided per opaque scope, provider and operation; no Default |
| `Observer::capture` | Opens one `CaptureSink` per physical exchange when capture is on; events borrow heads, chunks and WS frames, sequenced across both directions |
| `Observer::usage` | Called exactly once per request with usage enabled, including cancelled and failed ones, after the stream/socket finished |
| `Observer::trace` | Borrowed `TraceEvent`s for attempt/exchange start and finish and credential refresh; nothing is formatted when trace is off |

Recording never rewrites, reorders or delays the delivered stream, and a sink
failure is the host's problem rather than the request's. Downstream capture and
linking to these upstream exchanges stay with the host, correlated by request,
attempt and capture IDs. Core does not yet call any of these methods.

## Protocol capabilities

[capability.rs](src/capability.rs) implements the host capability traits that
`gproxy-protocol` adaptation flows are generic over, so a multi-call adaptation
never receives a raw client:

| Type | Trait | Binding |
|---|---|---|
| `AttemptUpstream` | `Upstream<Target = OperationKey>` | One attempt's provider, pinned credential version and client; the target is only the native operation to dispatch |
| `ProtocolState` | `StateStore<Scope = StateScope>` | Store ProtocolState rows; scope is caller scope + provider + optional conversation key, serialized by core |
| `Resources` | `ResourceAccess<Scope = ResourceScope, PublishedHandle = PublishedHandle>` | Store ResourceBinding/FileObject rows and the file backend; the scope carries the permitted `ExecutionTarget`, which may name a source provider other than the request target |

Each instance takes explicit `CapabilityLimits` from the execution path that
constructs it; there is no unlimited default. `AttemptUpstream::send/connect` must
apply the provider's operation URL, reject absolute URLs and source authentication,
and route through the observation wrapper. All methods currently return a
`CapabilityError` of kind `Unsupported` stating that the function is not implemented.

```sh
cargo test -p gproxy-core
cargo clippy -p gproxy-core --all-targets -- -D warnings
cargo clippy -p gproxy-core --target wasm32-unknown-unknown --all-targets -- -D warnings
```

Tests cover concurrent publication and retained in-flight views. They do not
establish a working selector or gateway. See [crate boundaries](../../design/crates.md).
