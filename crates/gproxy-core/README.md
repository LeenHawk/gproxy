# gproxy-core

English | [简体中文](README.zh-CN.md)

The provider execution engine, without downstream concerns. The upper layer
resolves routes and model aliases, authenticates callers, evaluates policies and
admits requests; it then hands core a Store, a Cache and one
`ExecutionTarget` (provider, upstream model, permitted credentials). Core does
protocol conversion, request/response rewriting, credential selection, refresh
and availability, upstream calls over HTTP and WebSocket, stream forwarding,
observation and settlement, continuation state and resources for multi-call
adaptations, agent session assignments, account quota observation, usage
extraction and local token estimation.

| Module | Role |
|---|---|
| `builder` | `Core::builder(store)`: cache and secret codec required; Store-backed observer by default, explicit override supported; channels registered into a `ChannelRegistry`; optional client pool and file storage |
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
| `quota` | `QuotaHeaders`/`QuotaQuery` observations into `credential_quota_cycles` and exhaustion blocks; Counted dimensions metered in Store `counted_windows` rows |
| `budget` | Caller budgets in USD: owners named by the host, lazy `quota_windows`, pre-attempt rejection, settlement, manual reset and status |
| `pricing` | `PriceBook`: the Store price rules, rates and tiers compiled per snapshot; every exchange is priced at settlement |
| `session` | Agent session assignments: stay bound while usable, reserve a new generation on durable failure, activate or fail from the preparing attempt |
| `estimate` | Local token estimation for exchanges the upstream did not meter |
| `observe` | The host's settlement/capture/trace funnel with a pre-work policy query |
| `keys` | The one cache key grammar and the invalidation topic |

## Snapshot and data

`CoreData` holds providers, credentials, reusable rewrite sets, the execution
limits, the estimator, the enabled caller budgets and the price book. It does
not hold routes, exposed-model aliases, identity tables, permissions, OAuth
client allowlists, subscriptions or admission rules; those belong to the upper
layer, together with route affinity and cross-provider balancing.
`publish_snapshot` accepts only a newer revision;
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

The client is resolved per credential, first match wins, whole configuration
replaced (no field merging): the credential's `connection_profile_id` → the
provider's → the Setting's default profile → the channel's
`BaseChannel::default_connection_for(Request)` →
`ConnectionConfig::default()`. A referenced profile that is missing or invalid
fails the whole assembly.

Codex defaults to reqwest; Claude Code API calls use wreq without emulation.
Claude Code cookie login (including cookie-based token recovery) and Claude Web
use browser-emulated wreq. Explicit credential/provider/global profiles still win.

Availability is one `CredentialBlocks` cache payload per credential, rebuilt
from `credential_blocks` rows on load. Each block names a channel `QuotaScope`,
an optional operation, an `until_ms` and a `BlockSource`: a Reported dimension
observed exhausted (with its persisted cycle), a Counted window used up, upstream
rate limiting, or consecutive failures. Streaks are per scope and operation, so a
broken model neither hides behind nor poisons a healthy one. See
[credential availability](../../design/core-credential-availability.md).

`CredentialStrategy` supports `round_robin` and `earliest_reset`. The latter
prefers the earliest future reset among applicable observed quota windows;
ties rotate and missing observations fall back to round-robin. Blocked credentials
remain excluded. The independent provider config `session_affinity` reuses an
eligible successful pin before applying the selection strategy. Legacy `sticky`
and `round_robin_affinity` retain round-robin with affinity by default; an explicit
`session_affinity` overrides that default. Pins are scoped by caller and provider.
See [session identity and selection](../../design/session-identity.md).

Every channel exposes the provider config `forward_thinking` (default `true`).
Set it to `false` to remove carried thinking/reasoning blocks and signatures from
Claude, Responses (including WebSocket), Chat, and Gemini request history before
conversion and upstream dispatch. Text and tool data are preserved, as are the
controls that request thinking for the new response.

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
| `call_service` / `connect_service` | [service.rs](src/service.rs): the channel's `ChannelServices` (vendor CLI calls without an `OperationKey`) under a `ServiceView`. `Caller` (any role) renders the caller's own picture from the gateway's accounting for explicit `ServiceRequest::user_id` and the resources bound to `scope` — a member never sees any credential's state; `Pool` (admin) merges the target's credentials into one synthesized account; `Credential(id)` (admin) forwards raw with that credential's auth. Core never decides who is an admin of what: the host expresses the org boundary by choosing `target.credentials`, and `CallerRole::Admin` means admin over that set. Core supplies the facts (`TargetCaller`: usage rows keyed by the explicit user ID, the current windows of the budgets named in `ServiceRequest::budgets`, quota cycles of the target's credentials, `resource_bindings`), picks the first usable credential (or the named one) and item routes follow the credential recorded in the binding. A wrong role is `Forbidden`, a channel without services is `Channel(UnsupportedService)`. Runs **outside the funnel**: no attempt, usage, capture or retry |

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
observation, per-unit response rewriting and cancellation; ending or dropping it settles the request exactly once, and usage
is recorded before anything else can settle. `Execution` can only be built with
the funnel's `Settled` proof. WebSocket operations run the same loop for the
handshake; an established socket is captured, metered and rewritten in both
directions and settles when it ends or is dropped.

## Conversion

`convert::route` resolves an exact `(operation, incoming dialect)` mapping to
`Passthrough`, `TransformTo { target: OperationKey }`, `Local` or `Unsupported`.
Channel declarations supply defaults. A provider's `action = "routing"` row
stores typed mappings keyed by incoming dialect; editing or resetting one key
preserves the others. Legacy `dialects` rows are read as support sets, never as
preference lists. A channel can declare an explicit conversion target; otherwise
only an unambiguous supported edge is selected. Ambiguous mappings stay unsupported.

Local handlers expose the configured model directory and count request text with
the selected tokenizer without making an upstream call. Channel-local model
methods retain their own credential-backed implementation. An explicit streamed
to buffered generation mapping synthesizes the client's stream from the result.
The reverse mapping collects the native stream into a complete client response;
Codex HTTP generation defaults to streaming Responses for both response modes.
For SSE calls that may use buffered generation, the SDK sends a comment heartbeat
every 15 seconds while waiting. Fast failures retain their HTTP status; after the
first heartbeat, late failures are SSE error events. Retries and settlement remain
inside the pending execution. Gemini JSON-array responses receive no SSE comments.

Conversion runs protocol's adaptation flows over `AttemptUpstream`, so every
native call inside a flow is captured, metered and rewritten like a passthrough
call, and a rejected native answer re-enters the same classification.

| Family | Coverage |
|---|---|
| Generate | All twelve dialect pairs, buffered and streamed; Chat `n` / Gemini `candidateCount` fan out into journaled child calls; Responses HTTP/SSE, Chat, Claude and Gemini clients over a Responses WebSocket upstream; managed Responses sessions over Responses, Chat, Claude or Gemini HTTP streams, with context warmup, interruption, steering and injection |
| Models | List and get across OpenAI, Claude and Gemini using provider model supplements |
| Count tokens | Claude and Gemini targets; OpenAI as a target is deliberately unsupported |
| Embeddings | OpenAI ↔ Gemini, single and batch |
| Guardian, compact, memory | Any of the four targets; guardian streams are refused |
| Files | Retrieve, list, delete, content and multipart upload including Gemini's resumable protocol |
| Images | OpenAI create/edit over Gemini or the Responses image tool; URL delivery through the host's `PublicationUrl`, refused before any call without one |
| Video | OpenAI native video over Veo with job state in `ProtocolState` |

## Credential lifecycle, quota and sessions

`refresh_credential` is an explicit account operation; the attempt loop calls
it `IfNeeded` before pinning expiring material and `Force` after a 401/403.
Quota has two tracks: Reported dimensions receive values from headers, queries
or exhaustion replies and block until the upstream period end (or one window
derived from the dimension); Counted dimensions are metered in Store's
`counted_windows` rows with an atomic fits-under-the-limit update, requests
before the exchange (a refused charge skips the credential without sending)
and tokens and priced USD cost after usage settles, and block until the window
ends. Counts are shared by every instance and survive restarts. Entries that
match no declared dimension are still persisted as cycles.

Operators can add limits the channel does not know about as `quotas` rows
owned by a `credential` (`owner_id` = credential id) or a `provider`
(`owner_id` = provider id, covering every credential of the provider unless a
credential row with the same `window_key` overrides it): `metric` `requests`
with unit `count`, or `cost` with unit `USD`; `period` `5h`/`1d`/`7d` (fixed
windows aligned to `anchor_at_ms`, epoch 0 when unset), `1m` (UTC months) or
`total` (never resets); an optional `model_pattern` glob. Assembly appends
each as a Counted dimension `limit:{quota_id}` after the channel's own, so
enforcement is the machinery above: requests are charged before the attempt,
cost after settlement (an unpriced exchange charges nothing), and a full
window writes a `Counted` block until the window ends. A literal
`model_pattern` becomes the dimension's `Models` scope; a glob is checked at
charge time and a block then names the charged model. Invalid rows are
logged and skipped. These are upstream-side limits, not caller budgets:
they never see `RequestContext::budgets` and are never settled.
`Core::credential_limit_status(credential_id, now)` reports each limit's
current window with `used` and `limit`; `Core::reset_credential_limit(quota_id,
now)` sets the row's `anchor_at_ms` to `now`, deletes the dimension's counted
windows, clears its `Counted` blocks from Store and cache and reloads the
affected credentials, so counting restarts at once (fixed windows realign to
the new anchor once a reloaded snapshot is published; `total` starts a fresh
permanent window).

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
half the response bytes. The prompt text is read through the protocol wire
types of the exchange's native `(operation, dialect)` pair — system prompts,
messages, tool calls and results, tool declarations — for Claude Messages,
OpenAI Chat, OpenAI Responses (HTTP and websocket envelope), OpenAI
count-tokens, guardian, compaction, memory and embeddings, and Gemini
generate, count-tokens and embeddings. Base64 media, file ids, URLs and
encrypted blobs are never counted. There is no key-name fallback: a pair
without an extractor, or a body the wire types reject, gets no input estimate
at all (the output estimate still applies). Estimates are `Partial` and carry
`dimensions["estimated"] = "true"`; reported values are never overwritten and
rejected answers are not estimated. `UsageState::Skipped` means the request's
policy disabled usage, distinct from an upstream that reported nothing.

## Budgets and pricing

A budget is a `quotas` row with metric `cost` and unit `USD`, owned by one
`(owner_kind, owner_id)`. Kinds are strings the host defines (suggested:
`user`, `api_key`, `subscription`, `pool`, `team`, `org`, but any level of the
host's organisation works); core matches them verbatim and knows no hierarchy
between them. The host passes the chain of owners a request spends for as
`BudgetOwner { kind, id }` values in `RequestContext::budgets` (and
`ServiceRequest::budgets` for the `Caller` service view), and the request
charges the whole chain: a personal key passes `[api_key:k1, user:u]`; a team
key passes `[api_key:k2, user:u, team:t, org:o]` and is rejected when any of
the four is exhausted, naming that quota; the same user's key in a second org
passes `[api_key:k3, user:u, team:t2, org:o2]`, which shares nothing with `t`
and `o`. Whether team-key usage also counts against the user is the host's
choice: it does when the host includes `user:u` in the chain. Core resolves
the chain to the enabled budgets of those owners whose `model_pattern` (a
`*`/`?` glob over the whole upstream model name, blank for all) covers the
target model. Every applicable budget must have
room (AND): before the first attempt each one's current window is loaded — or
opened lazily as a `quota_windows` row keyed by `(quota_id, starts_at_ms)`,
carrying a snapshot of the quota — and a window with `used >= limit_value`
fails the request with `CoreError::BudgetExhausted { quota_id, window_key,
resets_at_ms }` before any credential is touched; the funnel still delivers a
`Failed` usage report and, with tracing on, `TraceEvent::BudgetRejected`.

Periods are `5h`, `1d` and `7d` (fixed windows aligned to `anchor_at_ms`, epoch
0 when unset; any other period with `period_seconds` works the same), `1m` (UTC
calendar months) and `total` (one permanent window, `ends_at_ms = None`).
Expired windows stay as history; the next request opens the next one.
`Core::reset_budget(quota_id, now)` closes the open window at `now`, sets the
quota's `anchor_at_ms` to `now` and opens a fresh window starting there (a whole
period for fixed windows, to month end for `1m`, forever for `total`); the new
anchor governs later windows once a reloaded snapshot is published.
`Core::budget_status(owners, now)` reports every budget of the owners with its
window, `used`, `limit` and `resets_at_ms`, for hosts and consoles.

Cost is core's job. At settlement every exchange is priced from the snapshot's
`PriceBook` (`price_rules` with their `price_rates` and `price_tiers`): the
provider's rules precede global ones, the lowest `(priority, id)` wins among
those whose `model_pattern` glob and optional operation fit; tiers are picked by
`actual_service_tier` and `min_prompt_tokens`; token kinds are priced per
million, other metrics by their rate row, conditions matched against the usage
dimensions (see [pricing.rs](src/pricing.rs) for the mapping). The result lands
on `ExchangeUsage::cost` and `UsageReport::cost` for the Observer. An exchange
no rule covers costs 0 and carries `dimensions["unpriced"] = "true"`. The
request's cost is then settled once per applicable budget window — one
settlement per owner in the chain — idempotent by `request_id`
(`quota_settlements`), only into budgets whose unit is the cost's currency. Because cost is known only after the exchange, a budget can be
overrun by at most one request.

## Observation

Core defaults to `StoreObserver`. The snapshot's `observation` settings separately
control upstream metadata/body capture, usage retention, and pricing/settlement.
`enable_usage = false` disables request usage rows without disabling settlement;
`enable_settlement = false` disables pricing and quota charges while usage may
still be recorded. Settings take effect on configuration reload for new requests.
Supplying `.observer(...)` replaces the built-in persistence implementation.

Each physical exchange is persisted independently, including retries and multiple
HTTP sends inside one channel invocation. Stream chunks and WebSocket messages
are ordered `upstream_events`; response errors and interruptions keep their prior
bytes and partial usage. Each metered physical upstream call has one usage row. The completion funnel
awaits all exchanges before persisting usage, even if its response body, execution
future or completion waiter is dropped. Request attribution is explicitly supplied by the host, never inferred
from the opaque scope. Downstream capture remains a host concern; core writes call associations to
`capture_links` alongside capture and usage.
See [the observation contract](../../design/core-observation.md) for the switch
matrix, storage representation and process-crash boundary.

## Protocol capabilities

| Type | Trait | Binding |
|---|---|---|
| `AttemptUpstream` | `Upstream<Target = OperationKey>` | One attempt's provider, pinned credential version and client; owned and cloneable so lazily started fanout children outlive the attempt frame |
| `ProtocolState` | `StateStore<Scope = StateScope>` | Store ProtocolState rows; scope is caller scope, provider and optional conversation |
| `Resources` | `ResourceAccess<Scope = ResourceScope>` | Publications in `resource_bindings` and `file_objects` through file storage; `Id` reads resolve a same-scope publication first, then the scope provider's files API; `Url` reads are fetched by an anonymous client under the host's `FetchPolicy` (`resolve` is the policy check, `read` the GET) |
| `ChannelStateStore` | `gproxy_channel::ChannelState` | The same ProtocolState rows, scoped to one provider and one credential, handed to the channel as `OperationContext.state`; the channel picks keys and never sees the scope |

Reading by URL (request media given as links: `image_url`, Gemini `file_uri`,
Claude URL sources) is a server-side fetch on the caller's behalf, so it runs
under a `FetchPolicy` (`CoreBuilder::fetch_policy(Arc<dyn FetchPolicy>)`). The
policy sees the URL and the addresses the resolver returned for its host and
answers `Allow` or `Deny(reason)`; it is asked for the first request and again
for every redirect (at most three hops), so a public link cannot bounce the
fetch onto a private one. `DefaultFetchPolicy` applies when nothing is set:
http/https only, and no loopback, link-local, RFC 1918 / `fc00::/7`, unspecified,
broadcast or multicast address, IPv4-mapped forms included, whether the address
is the URL's literal host or a resolver answer. A single-user host whose network
is the caller's own network opts out with one line,
`.fetch_policy(Arc::new(AllowAllFetchPolicy))`; `AllowlistFetchPolicy::new(["cdn.example", "*.media.example"])`
trusts named hosts only. The fetch goes through a plain client of the core
pool's default profile, never the scope's credential client, so no provider
auth reaches an arbitrary origin; the body is bounded by the read limit
(`Content-Length` is checked first) and `Content-Type` / `Content-Disposition`
fill the returned metadata. Two residual risks stay with the host: the
outbound client resolves the name again when it connects, so core cannot pin
the checked addresses (a name that rebinds between the two lookups escapes the
check — front the fetch with an egress proxy or an allow-list of names you
control if that matters), and on wasm32 there is no resolver, so the policy
judges names alone (literal IP hosts are still checked) and the platform's
`fetch` follows redirects itself.

Publishing by URL (images `response_format: url`) needs the host's link builder,
`CoreBuilder::publication_url(Arc<dyn PublicationUrl>)`. Core has no public HTTP
surface, so it owns the bytes and the binding id while the host mints the link:
`url_for` is called with the id core is about to record, before anything is written,
so a `None` answer (or a core without a builder) is `Unsupported` with no row and no
object behind it. The host serves the link on its own route by calling
`Core::read_publication(id)`, which returns the metadata and stored bytes for any
live publication regardless of scope (the route has already authenticated whoever
holds the link) and `None` once it expired, was released, or the object is gone
from the backend; any other backend failure is `CoreError::File` with the opendal
error inside. `Core::delete_publication` tombstones it early. The images family checks for the builder before the first
upstream call, so a host without one never pays for images it cannot deliver.

Every channel call also carries `OperationContext.instance_id`, the identity of this
host process (`CoreBuilder::instance_id`, random by default). A channel that must keep a
live upstream connection between two requests (claudeweb parks the completion stream at
`tool_use`) records the holder with the continuation. When a later request lands on
another process the attempt fails with `CoreError::ContinuationElsewhere { instance_id }`:
nothing is retried, no failure is recorded against the credential, and the continuation
stays where it is. A multi-instance host either routes the conversation to that instance
or keeps the conversation sticky in the first place; a single instance never sees it.

## wasm32

SQLite/filesystem integration tests run on native targets only, matching their
platform-specific dev dependencies. The wasm check includes the library and
portable test targets; it does not execute tests in a browser.

The engine runs on wasm32-unknown-unknown with the same API: timers and
background tasks come from the JS event loop, outbound transport from
gproxy-client's `fetch`/`workers` features or a host-injected `OutboundClient`
(see the client README), file storage from gproxy-file's `s3` feature (R2 or
any S3-compatible store over fetch; the local filesystem backend is native
only), and estimation from the same tokenizer stack. The bundled DeepSeek
vocabulary is the `bundled-vocabulary` feature (on by default, about 4 MiB in
the binary); without it, models with no catalog vocabulary file use the
character estimate. Tests run natively only.

```sh
cargo test -p gproxy-core
cargo clippy -p gproxy-core --all-targets -- -D warnings
cargo clippy -p gproxy-core --target wasm32-unknown-unknown --all-targets -- -D warnings
```

Tests run against an in-memory SQLite Store, the memory cache and scripted
channels/clients; they establish the engine's contracts, not a working gateway
or a real provider. See [crate boundaries](../../design/crates.md).

The StoreObserver background writer coalesces capture chunks per direction at
about 64 KiB or one second idle, and flushes on finish, explicit flush and sender
drop. Redaction runs before enqueueing; compression and request-body CDC run only
in the writer. Forwarded stream bytes and the Observer/CaptureSink contracts are
unchanged. The store's capture hydrator restores original chunk/frame boundaries,
sequences and timestamps from encoded segments.
