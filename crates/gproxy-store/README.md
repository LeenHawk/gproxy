# gproxy-store

English | [简体中文](README.zh-CN.md)

SeaORM 2 entity definitions for GPROXY v4. **This is an entity review draft**, not
an implemented repository or an applied database migration.

Start at [`src/entity/mod.rs`](src/entity/mod.rs). Entities are grouped by domain,
with one entity per file:

| Directory | Entities |
|---|---|
| `upstream` | Provider, Credential, Model, ProviderModel, OperationRule |
| `routing` | ExposedModel, Route, RouteMember |
| `identity` | Organization, Team, OrganizationMember, TeamMember, User, ApiKey, UserSession, Permission |
| `oauth` | Client, Grant, Code, Token, Device |
| `subscription` | Pool, PoolMember, Plan, PlanLimit, Subscription |
| `limits` | RateLimit, Quota, QuotaWindow, QuotaSettlement, CredentialQuotaCycle |
| `pricing` | PriceRule, PriceRate, PriceTier |
| `usage` | UsageRecord, CaptureRecord, CaptureLink, CaptureEvent |
| `resource` | FileObject, AgentSession, AgentAssignment, ResourceBinding, ProtocolState |
| `config` | Setting, ConnectionProfile |

Fields, primary/unique keys, relations, and delete actions are declared directly
with SeaORM attributes. `schema(backend)` registers all entities with SeaORM's
schema builder.

The draft covers upstreams and routing, users and API keys, policies and quotas,
pricing, historical usage and calls, file metadata, resource bindings, protocol
continuation state, and settings.

Review decisions currently expressed in the code:

- Business IDs are caller-assigned strings; timestamps use Unix milliseconds.
  Global settings uses row `id = 1`, with explicit network, execution, tokenizer,
  logging, storage-selection, maintenance and portal fields.
- Providers are global. Each credential references a provider and has one owner:
  an organization, a team, or a user. The owner IDs are separate from provider configuration.
- Named connection profiles hold backend, proxy mode/URL, wreq emulation and
  decompression, redirect, retry and connection-pool parameters. Optional `connection_profile_id` references resolve
  Credential → Provider → Setting → built-in reqwest/direct defaults. `None`
  inherits the complete next profile; explicit direct/system modes belong to a
  profile, not to a nullable URL. Referenced profiles use `ON DELETE RESTRICT`.
  There is no profile version. `gproxy-client` caches by effective parameters;
  host CRUD, validation, inheritance and execution wiring are not implemented here.
- A team belongs to one organization. OrganizationMember and TeamMember store
  scoped `member`/`admin` roles, so one user's role can differ between groups.
- Organization credentials can be used by its members and descendant team users;
  team credentials can be used by team members. Viewing, editing and deleting
  shared credentials require the administrator role of their owning scope.
  Entity definitions record these relationships; API authorization is not implemented here.
- Provider models may reference a global model; deleting catalog metadata clears
  that optional reference.
- Public model names map to routes containing provider/upstream-model members.
  Routing definitions have no organization, team or user ownership.
- Permissions/rate limits currently target users or API keys. A quota targets
  exactly one user, API key, subscription or subscription pool; the future write
  layer must enforce ownership consistency.
- Configuration-owned rows use the declared delete actions. Historical identity
  references have no configuration foreign keys. Quota settlements refer to quota
  windows, independently of removable usage detail records.
- File content stays in filesystem/S3 storage. File entities contain locators
  and metadata. Custom vocabulary configuration references a file object.
- Amounts use `Decimal(28,12)` as a proposed business representation. The current
  D1 adapter does not implement that mapping; decimal storage and precision must
  be settled before these entities are used for D1 data operations.
- Reusable mutation rule sets, audit events, and derived usage rollups are outside
  this first entity draft.

## Routing structure

[`ExposedModel`](src/entity/routing/exposed_model.rs) maps a globally unique public
model name exactly to a [`Route`](src/entity/routing/route.rs). Multiple public names
can share a route. Routes contain a name, enabled state, a round_robin/weighted/failover
strategy and a positive max_attempts including the first attempt, bounded by the
global attempt limit during execution.

[`RouteMember`](src/entity/routing/route_member.rs) selects a provider and explicit
upstream model, with tier, positive weight and enabled state. Prefer lower available
tiers; balance members in the preferred group, then select a credential using the
provider strategy. No model-catalog FK is required. The current v3 API/execution
path no longer uses the legacy schema's pinned credential_id or priority columns,
so they are not included here.

Namespace is derived from the first segment of a public name: coding/fast is
indexed globally by its full name and as fast inside coding. There is no Namespace
table. Named entry points resolve namespace, then route name, then provider name.
This model-to-route mapping is distinct from alias string rewriting.

Deleting a route cascades to its members and public mappings; deleting a provider
removes its members. These definitions have no organization/team/user ownership.
Nonempty names, positive weights/budgets and runtime selection are future write-layer
and execution contracts, not behavior implemented by these entities.

## Pricing structure

- [`PriceRule`](src/entity/pricing/price_rule.rs) selects global/provider pricing
  by upstream model and optional operation, and defines the currency. Provider
  rules precede global rules; within a scope, lower `(priority, id)` wins.
- [`PriceRate`](src/entity/pricing/price_rate.rs) stores the metric, quantity unit,
  denominator and price. Independent row IDs allow multiple rates for one metric,
  conditioned on dimensions such as image size, quality or tool name. The first
  fully matching conditional row by `(priority, id)` replaces the unconditional
  fallback; rates for the same metric are not added together.
- [`PriceTier`](src/entity/pricing/price_tier.rs) expresses context thresholds,
  service tiers and their combinations with explicit per-million token prices.
  Select the highest reached context threshold, then the highest reached actual
  service-tier threshold; lower `(priority, id)` breaks ties. Explicit service-tier
  prices win; inherited context prices receive the service-tier multiplier.
  `None` inherits and zero is free. Prompt thresholds count ordinary input,
  cache reads and cache writes once each.

[`metric.rs`](src/entity/pricing/metric.rs) names built-in metrics; custom keys
remain supported:

| Category | Quantities | Units |
|---|---|---|
| Text / embedding | Input, output, cache reads, 5min/30min/1h cache writes, reasoning | Tokens, usually per million |
| Image | Input/output tokens, generated images | Tokens or count; size/quality conditions |
| Audio | Input/output tokens, cached audio input, duration, speech text | Tokens, seconds or characters |
| Video | Input/video tokens, duration, generated videos | Tokens, seconds or count; resolution conditions |
| Rerank | Input tokens, search units | Tokens or count |
| Tools | Web search/fetch, file search, code execution sessions, other tool calls | Count; tool-name conditions |
| Requests | Per-request fees | Count |

The price formula is `quantity * value / unit_quantity` in the rule currency.
Denominators must be positive; prices, thresholds and multipliers nonnegative;
conditions must be nonempty objects of scalar values. These are future write-layer
validation contracts, not implemented business checks. Deduct cache reads from
ordinary input, avoid billing reasoning/media subsets twice when aggregate token
prices already cover them, and do not unconditionally add per-image and per-token
charges.

These entities and metric names do not implement extraction, rate selection or
settlement. Adding a metric does not imply every upstream reports its quantity.

## Downstream/upstream exchanges and streaming

`UpstreamCall` is merged into [`CaptureRecord`](src/entity/usage/capture_record.rs).
One row holds a physical exchange on one side, including call metadata and both
request and response. `side` distinguishes downstream from upstream.

| Entity | Responsibility |
|---|---|
| CaptureRecord | HTTP exchange, WS connection or WS business turn, with request and response together |
| CaptureLink | Many-to-many downstream/upstream edges, with order local to each downstream |
| CaptureEvent | Ordered stream chunks or WS messages, including direction and observation time |

HTTP request elements are method, URL/path, raw query, headers and body; response
elements are status, headers and body. The query is stored separately and headers
use `[name, value]` pairs to preserve repetitions, matching the actual WireRequest
and WireResponse contracts.

Bodies stay in the database without FileObject references. `Buffered` uses inline
request/response body columns; streaming leaves them unset and appends events.
Concatenate payloads in sequence per direction to reconstruct SSE, NDJSON, JSON
arrays or byte streams, including their delimiters. Chunks are not calls. Capture
completeness is tracked separately for request and response; exchange state tracks
completion, failure or cancellation independently of HTTP status. Body events need
not be stored when body logging is disabled. Logging redaction applies to URLs,
queries, headers and bodies.

Edges support D1-U1 (one-to-one), D1-U1/D1-U2 (fanout/retry), D1-U1/D2-U1
(sharing), or any many-to-many combination. Each actual retry has its own upstream
record; shared upstream payload/usage is stored once. No single common request ID
is forced on upstream records. An edge's sequence is local to its downstream.
Deleting an endpoint removes its edges, not the opposite endpoint. A downstream
error without any upstream call is independently representable.

WS uses `WsConnection` for handshake/lifetime and `WsTurn` for each business turn,
linked to the same-side connection by `session_id`. Only the handshake carries
HTTP method/headers/status; later turns do not invent HTTP envelopes. All WS message events belong to the connection, independently of its handshake
body. An optional turn_id associates a business message with its turn; control
and unassigned messages leave it unset. Store each message once; the primary key
(capture_id, sequence) enforces unique connection-wide order across interleaved turns. This logs
application messages, not TCP packets or WS fragments. Leave messages at connection
scope when protocol evidence is insufficient to assign a turn.

Edges connect HTTP exchanges/WS turns, allowing HTTP-to-WS and WS-to-HTTP as well
as aggregation/fanout across any number of connections. Connection reuse does not
imply reuse of a business invocation. Writers must enforce endpoint directions,
same-side turn/session binding and event ordering; these are not automatically
validated by the entity definitions.

UsageRecord.request_id identifies the downstream HTTP exchange or WS turn, with
independent log retention. Native upstream metrics belong to the physical exchange
and must not be summed again for each edge. Downstream cost allocation is an
explicit settlement policy; edges imply neither equal splitting nor repeated full
charges.

These remain entity definitions. Capture integration, WS turn identification,
shared-call settlement and log query APIs are not implemented here.

## OAuth

[`oauth`](src/entity/oauth/mod.rs) models GProxy issuing authorization to downstream
clients, separately from logging in to upstream accounts:

| Entity | Contents |
|---|---|
| Client | Public client_id, name, redirect URIs, enabled/soft-deleted state; no client secret |
| Grant | User, internal API key, client, scopes, ID-token identity, revocation and login/refresh history |
| Code | Authorization-code SHA-256 hash, redirect URI, PKCE S256 challenge, expiry and consumption receipt |
| Token | Access/refresh token hashes, grant, expiry, rotation receipts and revocation |
| Device | Secret device-code hash, user code, client/scopes, grant, approval/denial/consumption and optional sealed Codex result |

Client.id is the public OAuth client_id. Grants bind GProxy users and one internal
API key of kind oauth, not a provider/credential. Internal keys carry policy/usage
identity but cannot authenticate/export as ordinary API keys. OAuth requests use
the normal admission, routing, settlement and capture path with current user
permissions intersected with granted scopes; they grant no Console admin access.
UserSession remains a separate Console login session.

Code/refresh consumption, token insertion and session statistics must commit in
one atomic operation. consumed_by is the replacement refresh-token hash used as
a consumption receipt. Writers must check key owner/kind and device/grant
client/scope consistency. Each request and new WS turn must recheck expiry,
revocation, client/grant/user/key state. Entities do not implement these checks.

Use revoked_at/deleted_at for revocation and client deletion to retain history;
reactivating a client must not restore old grants. Physical user/key/client deletion
cascades through grants to codes/tokens/devices for final cleanup. Issued bearer
tokens are stored only as hashes. Compatibility ID tokens are produced by the issuer;
signing keys remain host configuration. The authorize endpoint echoes state; the
ordinary authorization-code table stores a challenge, not the PKCE verifier.

Upstream OAuth stays in [`Credential`](src/entity/upstream/credential.rs).
OAuthCredentialSecret defines common plaintext inside its host-sealed secret:
access/refresh/ID tokens, type, scopes, refresh expiry and provider-specific fields.
expires_at_ms tracks access expiry. Refresh conditionally replaces secret/expiry
against version and increments it, avoiding stale overwrites. Encryption and provider
refresh adapters are not implemented. Short-lived upstream state/verifier/device
transactions belong in an expiring cache bound to initiator and provider; credential
ownership rules and the three-level connection-profile selection continue to apply.

Only entities and the serialized credential shape are implemented. OAuth endpoints,
key management, atomic rotation, upstream login/refresh and client integration remain
future work.

## Subscription aggregation and allocation

[`subscription`](src/entity/subscription/mod.rs) models gateway-issued virtual subscriptions:

```text
Credential -> PoolMember -> Pool -> Plan -> Subscription -> API keys / OAuth sessions
                                      PlanLimit -> subscription-owned Quota
Pool/subscription Quota -> QuotaWindow -> QuotaSettlement
```

PoolMember binds a credential and a channel-defined canonical source_key for the
real upstream subscription. A source contributes to one pool only; downstream
allocations split its capacity. Token refresh does not create a new source, and
membership does not change credential ownership. Pool-owned quotas are provisioned
budgets; actual available capacity comes from member CredentialQuotaCycle observations.
Configured budgets are not upstream-reported remaining capacity.

Plans define gateway display names and Codex/Claude Code presentation fields.
PlanLimit defines default windows, dollar allowances (limit, in USD) and model scopes.
Issuance copies these into subscription-owned Quota rows with metric = cost and
unit = USD, optionally adjusting the dollar allocation.
Template edits do not retroactively resize/reset issued limits; changed terms use a
new plan. Subscriptions bind users and plans with start/expiry/enabled state. API
keys select a subscription; OAuth follows its internal key, so multiple sessions
share the same allocation. Key and subscription owners must match. A disabled or
expired subscription cannot fall back to unrestricted access. Plan.enabled controls
new issuance; Pool.enabled controls whether its capacity may continue serving work.

Group source capacities only by compatible metric, unit, model scope and window
class. Five-hour and seven-day constraints on one source are simultaneous limits,
not additive capacity. Preserve each source's reset time. Percentages without an
absolute basis remain unknown, not summed; stale/incomplete observations are not
zero consumption. Conversion and observation freshness require future aggregation
logic. Downstream allowances are always USD: token/cache/tool/media quantities
are priced in USD before charging. Subscription pricing rules must use USD; no
implicit currency mixing is defined. Upstream percentages/token limits describe
serving capacity, not dollar balances. Pool dollar budgets are explicitly configured
or separately estimated. Provisioning checks outstanding USD allocations against
compatible pool budgets; different windows/scopes cannot be naively summed.

Fixed downstream windows use subscription start/Quota.anchor_at_ms; calendar
windows use UTC and total quotas never reset. Upstream resets do not clear downstream
consumption. Client adapters use window_key (primary/secondary/etc.) to show the
subscriber's USD allowance, spent/remaining USD and reset (deriving percentages
from the dollar ledger where required), not all pool balances or
upstream account identities. Gateway plan labels do not change upstream entitlements.

Runtime candidates intersect route targets, pool members and existing credential
permissions. Subscription switches affect later requests/WS turns, not in-flight
work. Historical UsageRecord.subscription_id and upstream CaptureRecord.pool_id
retain attribution. Pool quota settlement uses physical upstream call IDs once;
subscription settlement uses downstream request IDs once. Shared-call allocation
still requires an explicit settlement policy. New OAuth logins must not silently
create fresh independent allowances.

Existing QuotaWindow/QuotaSettlement remain the only consumption ledger. Deleting
a subscription deletes its keys, OAuth grants and quota configuration, while historical
windows/settlements/usage survive. Prefer disabling/expiry to retain configuration.
Plans with issued subscriptions cannot be physically deleted. Only entities and
contracts are implemented: aggregation, provisioning, reservations, candidate
filtering and client subscription rendering remain future work.

## Cloud-agent affinity and exhaustion handoff

[`AgentSession`](src/entity/resource/agent_session.rs) identifies a stable logical
session by user, service/routing scope and downstream affinity_key, independently of
a captured WS connection. Reuse the same eligible credential until confirmed
exhaustion for the required workload; a transient rate limit is not proof of quota
exhaustion. After switching, stick to the replacement even if an older credential
resets. Routing/model permissions and subscription-pool eligibility still apply.
The downstream subscription and USD ledger do not change.

Each [`AgentAssignment`](src/entity/resource/agent_assignment.rs) stores a target,
previous active generation, reason/evidence references and preparation/activation/
replacement/failure/uncertain outcome. Never overwrite an old target. Resolve the
current target using session.active_generation, not the newest assignment. Reserve
switches by conditional version update and use the reservation revision as a
never-reused generation, including after failed attempts.

Prepare/rebuild/resume resources on the replacement as supported by the concrete
API. Only after successful preparation may an atomic commit validate the expected
session version, activate the new assignment, replace the old one and advance the
active pointer/version. Stale workers/callbacks cannot overwrite a newer selection.
New work waits while switching; existing calls retain their original assignment.
An uncertain upstream create outcome must be recovered through lookup or supported
idempotency, not blindly replayed after a timeout. These operations remain future
store/execution-layer work.

[`ResourceBinding`](src/entity/resource/resource_binding.rs) adds generation to its
unique public-resource key: ordinary resources use 0, agent resources use their
assignment generation. Stable public server/environment/session IDs can map to
new upstream IDs each generation while preserving old targets, dependencies,
metadata and timestamps. Token-kind public_id contains a host-generated downstream
bearer digest; upstream tokens live in a sealed secret, never public metadata.
Agent resource scope binds the logical session, not the changing provider. Writers
must validate owner/session/generation/target consistency. All associated handles
ultimately resolve through the same session's active generation.

Existing tasks/files/environments do not migrate merely by changing credentials.
New work uses the current generation; explicit reads/management of historical
resources use their original targets. Copy/rebuild/resume requires adapter support;
otherwise record the failure and block that continuation rather than claim seamless
migration. ProtocolState can hold checkpoints scoped to owner/session/generation,
with host encryption when needed.

In-flight WS/streaming calls pin CaptureRecord.agent_assignment_id; never splice a
second account into an established stream. Handoff affects prepared new calls or
supported reconnects. Capture references are historical. Credential/provider removal
cannot cascade-delete assignment or resource target history; missing configuration
means unavailable, not unrestricted fallback. Explicit session purge deletes its
assignment rows and clears binding assignment_id while retaining original target
and generation; such rows must not be treated as ordinary generation-0 resources.

This models v3 affinity/resource ownership and locally surveyed client APIs. Entities,
constraints and update contracts do not implement exhaustion detection, concurrent
handoff, resource recreation, token translation or live WS continuation. An inventoried
endpoint alone is not evidence of cross-account migration support.
