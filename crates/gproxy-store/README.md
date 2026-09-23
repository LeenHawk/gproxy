# gproxy-store

English | [简体中文](README.zh-CN.md)

SeaORM 2 entities, typed batch repositories and atomic persistence operations for
GPROXY v4. Native SeaORM connections and Cloudflare D1 share the same Store API.
Constructing Store neither opens a database nor applies schema changes.

## Store API

[`Store<C>`](src/store.rs) accepts a connection implementing
`gproxy_seaorm::BatchConnectionTrait`. All entity accessors share
[`Repository<C, E>`](src/repository.rs); normal CRUD is not hand-written per table.
Settings is the sole singleton accessor (`id = 1`).

```rust
use gproxy_store::{Store, entity::upstream::provider};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

let store = Store::new(connection);
let enabled = provider::Entity::find().filter(provider::Column::Enabled.eq(true));
let page = store.providers().page(enabled, 0, 50).await?;
let rows = store.providers().get_many(&["provider-id".to_owned()]).await?;
let control_data = store.load_control_data().await?;
```

| Methods | Contract |
|---|---|
| `create_many` | Caller-supplied keys; insert all rows and read database defaults in one batch |
| `get_many` | Preserve input order, duplicates and `None` for missing IDs; supports composite keys |
| `update_many` | Update explicitly `Set` non-key fields; preserve `NotSet`/`Unchanged`; return final rows or `None` |
| `delete_many` | Per-input affected-row counts |
| `query` / `query_many` | Ordinary SeaORM conditions, ordering and joins, returning complete entity models |
| `update_where_many` / `delete_where_many` | SeaORM conditional bulk mutations, with per-statement affected-row counts |
| `count_many` / `page` | Count before limit/offset; page count and items share one snapshot, with primary-key order tie breakers |
| `settings().get/update` | Read or upsert row 1; patch only explicitly `Set` fields |
| `load_control_data` | Coherent configuration-table snapshot as entity rows; no decrypted secrets or compiled runtime objects |

Each method executes one atomic batch through gproxy-seaorm. `get_many` splits key
predicates to fit D1's 100-bind statement limit within the same batch. Arbitrary
caller SQL is not split; database request/size limits still apply. Empty batches
perform no I/O. A `query` projection must include fields needed to decode its model;
custom projections and multi-entity results use gproxy-seaorm's batch API directly.
Page parent rows before loading one-to-many children.

SQL errors roll back transactional writes. Conditional zero-row writes return a
conflict/count and do not roll back other entries in the same batch. Domain methods
gate dependent writes using persistent receipts or version checks. There is no
automatic retry; an ambiguous commit result requires checking durable state first.
CRUD does not run SeaORM ActiveModel hooks or implement host authorization,
validation of every business field, cryptography, scheduling or network operations.
Use domain methods for protected transitions instead of bypassing them with CRUD.

[`operations`](src/operations/mod.rs) contains only atomic domain transitions:
credential refresh CAS and version-checked status changes, rewrite replacement/ordered loading, idempotent quota
settlement, expiring protocol-state CAS, OAuth issuance/rotation/revocation/device
approval/client retirement, and agent assignment reserve/activate/fail/current reads.
OAuth device polling/result delivery remains issuer work; generic queries can read
its persisted state. Subscription provisioning and price calculation remain core work.

## Counted windows and the database-backed cache

`counted_windows` rows record what core itself counts for a channel-declared
Counted quota dimension: one row per credential, dimension and window.
`Repository::charge_many` adds a charge only while `used + amount <= limit`,
atomically per row, and reports the usage either way.

`StoreCache` implements `gproxy_cache::Cache` over three tables
(`cache_entries`, `cache_counters`, `cache_permits`) for hosts that have no
memory-resident process and no Redis, such as edge isolates on D1 or libSQL.
Expiry is compared in SQL, so peers on one database agree on what is live.
It carries no notification transport: `subscribe` yields the initial
`ResyncRequired` and then nothing, so such a host reloads on its own schedule.

```rust
let cache: Arc<dyn Cache> = Arc::new(StoreCache::new(store.clone()));
```

## Initialization and versioned migrations

```rust
let store = Store::new(connection);
let report = store.migrate().await?;
// Log report.summary(), then initialize application data.
```

An empty database is created from the entity registry and receives a
`seaql_migrations` ledger. An existing managed database runs pending entries
of `gproxy_store::Migrator` through SeaORM. Tables without that ledger, or a
ledger containing migrations this build does not know, are refused before DDL.
`Store::sync()` remains a startup-compatible name for this operation; it no
longer infers `ALTER TABLE` statements from entity differences.

Run with one schema writer before serving requests. Construction does no I/O,
and migration does not create settings or administrators. Native connections
and D1 can drive the runner; a libSQL connection can create a fresh schema and
read an up-to-date one, but pending migrations need a native connection to that
database. D1 uses its batch/proxy adapter rather than interactive transactions.

Append migrations under `src/migration/mYYYYMMDD_NNNNNN_description.rs` and
append them to `Migrator::migrations()`. The baseline reads today's registry,
so subsequent migrations must also work when their schema change is already
present on a fresh install. Use `gproxy_seaorm::SchemaProbeExt` for backend
inspection. Never rewrite a released migration; append a correction. The full
convention is documented in [the migration module](src/migration/mod.rs).

`schema(backend).apply(&db)` is a low-level one-shot table creation API. Use
`Store::migrate()` for application startup so the ledger is recorded too.

## Exact amounts and schema changes

`FixedDecimal` stores units at scale 9: one USD atom is `$0.000000001`, with range
`-9223372036.854775808` through `9223372036.854775807`. JSON uses decimal strings.
Parsing/exact conversion rejects unrepresentable precision and overflow; calculate
with `rust_decimal::Decimal`, then explicitly use `FixedDecimal::rounded` once for
the complete settlement (ties to even). Rates, quantities and multipliers share
this representation; currency/unit semantics remain in the entity fields.

SQL columns are BIGINT. Entity `save_as = "decimal(20,0)"` and
`select_as = "char(32)"` carry integer atoms as text across D1's JavaScript boundary.
Database comparisons/order/addition remain numeric. Raw SQL and `col_expr` callers
must use the column's `save_as` conversion for FixedDecimal values. Settlement
checks nonnegative values and signed overflow before incrementing counters.

Compared with the previous schema draft, decimal columns now store scaled integers;
settlements/device approval add attempt receipts, agent sessions add a pending
assignment pointer, and database-size/refresh counters use signed SQL integers.
No existing database was migrated. Existing decimal data requires an explicit
conversion migration; schema sync does not convert its values or change its type.

## Validation

```sh
cargo test -p gproxy-store -p gproxy-seaorm
cargo clippy -p gproxy-store -p gproxy-seaorm --all-targets -- -D warnings
cargo clippy -p gproxy-store -p gproxy-seaorm --target wasm32-unknown-unknown --all-targets -- -D warnings
```

Permanent tests cover native SQLite CRUD/constraints, rollback, exact amounts,
state expiry, OAuth consumption and fenced agent handoff. The concurrency test
uses one SQLite connection; it is not multi-server stress evidence. Actual WASM
Store scenarios also passed against local Miniflare D1; see
[adapter validation](../gproxy-seaorm/VALIDATION.md). No live PostgreSQL/MySQL or
production Cloudflare validation was performed for these new Store operations.

Start at [`src/entity/mod.rs`](src/entity/mod.rs). Entities are grouped by domain,
with one entity per file:

| Directory | Entities |
|---|---|
| `upstream` | Provider, Credential, Model, ProviderModel, OperationRule, OperationEndpoint, RewriteRuleSet, RewriteRule, ProviderRewriteRuleSet |
| `routing` | ExposedModel, Route, RouteMember |
| `identity` | Organization, Team, OrganizationMember, TeamMember, User, ApiKey, UserSession, Permission, AuditEvent |
| `oauth` | Client, Grant, Code, Token, Device |
| `limits` | RateLimit, Quota, QuotaWindow, QuotaSettlement, CredentialQuotaCycle, CredentialBlock |
| `pricing` | PriceRule, PriceRate, PriceTier |
| `usage` | UsageRecord, CaptureRecord, CaptureLink, CaptureEvent |
| `resource` | FileObject, AgentSession, AgentAssignment, ResourceBinding, ProtocolState |
| `config` | Setting, ConnectionProfile |

Fields, primary/unique keys, relations, and delete actions are declared directly
with SeaORM attributes. `schema(backend)` registers all entities with SeaORM's
schema builder.

Gateway-issued plans, plan limits, subscriptions and resource pools have been removed.
Fresh databases omit their tables and binding columns. Existing databases retain
those unused tables and columns; startup does not purge historical data.

The schema covers upstreams and routing, users and API keys, policies and quotas,
pricing, historical usage and calls, file metadata, resource bindings, protocol
continuation state, and settings.

Review decisions currently expressed in the code:

- Business IDs are caller-assigned strings; timestamps use Unix milliseconds.
  Global settings uses row `id = 1`, with explicit network, execution, execution-limit
  (timeouts and byte caps core derives its finite limits from), `config_revision`,
  tokenizer, logging, storage-selection, maintenance and portal fields.
- Providers are global. Each credential references a provider and has one owner:
  an organization, a team, or a user. The owner IDs are separate from provider configuration.
  `status` (`active` / `dead`) plus `status_reason` is the durable lifecycle, distinct
  from the operator's `enabled` switch: `set_status_many` records a definitive refresh
  rejection with its reason under version CAS, and `refresh_many` returns the row to
  `active`. Selection needs only the bit; the reason is for people.
- Temporary limits are `CredentialBlock` rows: one per block with a channel `QuotaScope`
  JSON, optional operation, `until_ms` and a core `BlockSource` JSON. Core's cache is the
  hot copy and Store is authoritative across restarts; expired rows are pruned lazily.
  Deleting the credential drops its blocks.
- Named connection profiles hold backend (`reqwest`, `wreq`, `reqwest_native`), proxy mode/URL, wreq emulation and
  decompression, redirect, retry and connection-pool parameters. Optional `connection_profile_id` references resolve
  Credential → Provider → the channel's default connection → Setting → built-in reqwest/direct defaults. `None`
  inherits the complete next profile; explicit direct/system modes belong to a
  profile, not to a nullable URL. Referenced profiles use `ON DELETE RESTRICT`.
  There is no profile version. `gproxy-client` caches by effective parameters;
  repository CRUD is available; host validation, inheritance and execution wiring stay in core.
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
  one owner `(owner_kind, owner_id)`; kinds are host-defined strings (suggested:
  `user`, `api_key`, `team`, `org`) with no foreign key,
  so the host write layer must enforce ownership consistency and clean up. Two
  kinds are reserved by core for upstream-side limits rather than caller
  budgets: `credential` (one credential) and `provider` (every credential of a
  provider), with metric `requests`/unit `count` or metric `cost`/unit `USD`.
- Configuration-owned rows use the declared delete actions. Historical identity
  references have no configuration foreign keys. Quota settlements refer to quota
  windows, independently of removable usage detail records.
- File content stays in filesystem/S3 storage. File entities contain locators
  and metadata. Custom vocabulary configuration references a file object.
- Amounts use exact nine-place `FixedDecimal`, stored as signed BIGINT atoms.
  D1 uses textual transport and explicit casts; no floating-point conversion.
- RewriteRuleSet groups reusable rewrite rules; ProviderRewriteRuleSet attaches
  them to providers in order. RewriteRule stores regex, replacement, optional JSON
  dot paths and filters as explicit fields. Deleting a set cascades to its rules
  and attachments; deleting a provider removes only its attachments. Atomic rule replacement and ordered loading are implemented; rewrite execution stays in core.
- An API key is bound to at most one organization and at most one team. That
  binding is what the application layer derives the budget owner chain, the
  permission subject scope and the credential-visibility boundary from; it is
  never taken from a request header. Deleting the bound scope deletes the key.
- AuditEvent records one accepted management/portal operation: actor, source
  address, action name, target, outcome and a redacted `detail` summary. It has
  no foreign keys, because it has to survive the deletion of its actors and
  targets, and it is not part of any control snapshot. Derived usage rollups
  remain outside this persistence layer.

Rewrite rules now persist `target` (`body` by default, `header`, `query`) and
optional `target_name`. Header/Query name existing fields whose values are
rewritten, preserving duplicates; Query is request-only. `paths` and event filters
are Body-only. Target/phase/name validation belongs to rule compilation; Store
CRUD is not the rewrite executor. See [rewrite design](../../design/core-rewrite.md).
The added defaulted target column preserves old rules as Body during schema sync.

## Per-method URLs

`operation_endpoints()` provides ordinary batch CRUD for a provider's method URL.
Each row has `provider_id`, native `operation`, `dialect`, `transport` (`http` or
`websocket`), complete `url`, and `enabled`. The four selector fields form a unique
key. A provider can therefore use different URLs for OpenAI/Claude generation,
and separate HTTP/WS URLs for the same OperationKey. Streaming generation is a
separate operation and gets its own entry when needed.

URL overrides are independent of `OperationRule` action/remapping settings. The
new table is registered for schema sync and included in `load_control_data`;
deleting a provider cascades its endpoint rows. No existing operation-rule unique
constraint is changed. Delete/disable an endpoint to restore URL defaults.

Execution-data assembly validates operation/dialect and method URL requirements,
then builds ProviderData.operation_urls from enabled rows. Effective precedence:
method URL -> Provider base_url with the channel's path -> channel default URL.
A method URL is a complete address, not a base to append the default path to;
channel-defined path parameters remain that method's responsibility. Dynamic
returned download URLs and OAuth/service helper requests are not blindly replaced
by this setting. Core/channel execution wiring remains pending.

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
Nonempty names, positive weights/budgets and runtime selection are host write-layer
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
conditions must be nonempty objects of scalar values. These are host write-layer
validation contracts, not implemented business checks. Deduct cache reads from
ordinary input, avoid billing reasoning/media subsets twice when aggregate token
prices already cover them, and do not unconditionally add per-image and per-token
charges.

These entities and metric names do not implement extraction, rate selection or
settlement. Adding a metric does not imply every upstream reports its quantity.

## Downstream/upstream exchanges and streaming

`UpstreamCall` is merged into [`CaptureRecord`](src/entity/usage/capture_record.rs).
One row holds a physical exchange on one side, including call metadata and both
request and response. `side` distinguishes downstream from upstream. Optional
`initiator_request_id`, `attempt_id` and `attempt_ordinal` preserve the initiating
request and retry even without downstream capture; they are provenance, not
exclusive ownership of a shared upstream exchange.

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

Batch persistence is available. Capture integration, WS turn identification,
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
one atomic operation. `exchange_tokens_many` enforces this using a fresh random
`consumed_by` receipt for each attempt. `issue_many` checks key owner/kind and
device/grant client consistency; the issuer validates consent, scopes and redirect policy. `resolve_access_many` checks expiry,
revocation, client/grant/user/key state; core must call it for each
request and new WS turn and apply authorization/PKCE policy.

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

Atomic issuance, rotation, revocation, client retirement and upstream credential
CAS are implemented. OAuth HTTP endpoints, key management, upstream network
login/refresh and client integration remain host work.

### Hierarchical OAuth client-ID allowlists

Global `Setting`, `Organization`, `Team` and `User` each have a nullable
`oauth_client_allowlist` JSON string-array field, writable through existing
settings update/repository batch CRUD. Clients must also remain registered,
enabled and not soft-deleted in `oauth_clients`.

- `None`/SQL NULL means unconfigured and adds no restriction; an unconfigured
  global policy still requires client registration.
- `[]` admits no client for that scope. Other configured peers may still admit
  it through their same-level union.
- Union configured organization lists and configured team lists separately.
  Unconfigured peers do not widen that union to all clients.
- Intersect global, organization, team and user levels. A level with no configured
  lists adds no restriction. Children cannot widen parents; an empty global or
  user list denies everything.
- Organizations include direct memberships and parent organizations of the user's
  teams, even without separate organization membership rows. Admin roles do not
  bypass policy. IDs match exact, case-sensitive strings without wildcard syntax;
  non-array documents and non-string elements never match.

For example, global `[a,b]`, organizations `[a,c]` and `[b]`, and team `[b,c]`
admit only `b`. Adding user `[a]` yields no allowed client instead of overriding
its parents.

`oauth_clients().allowed_many(&[ClientAccess { user_id, client_id }])` provides a
batch preflight. `issue_many` (including device approval), code/refresh exchange,
and `resolve_access_many` recheck current policies in their own SQL statements.
Denial returns conflict/no identity without consuming tokens, approving devices or
creating dependent rows. Tightening policy blocks existing tokens at subsequent
checks; relaxing it can readmit unexpired, unrevoked tokens. Policy changes are
not permanent revocations. Core must resolve identity per request/new WS turn.

Dialect-specific JSON membership lives in
`gproxy-seaorm::json_array_contains_text`; Store only composes SeaORM conditions.
D1 uses SQLite's JSON extension. The current open-source `sea-orm 2.0.3` supports
SQLite, PostgreSQL and MySQL; MSSQL belongs to separate SeaORM X and is not wired
into this project. Unsupported backends return an explicit error. This adds four
nullable columns; existing rows inherit, and no database schema update is run
automatically.

## Cloud-agent affinity and exhaustion handoff

[`AgentSession`](src/entity/resource/agent_session.rs) identifies a stable logical
session by user, service/routing scope and downstream affinity_key, independently of
a captured WS connection. Reuse the same eligible credential until confirmed
exhaustion for the required workload; a transient rate limit is not proof of quota
exhaustion. After switching, stick to the replacement even if an older credential
resets. Routing/model permissions still apply. The existing usage ledger does not change.

Each [`AgentAssignment`](src/entity/resource/agent_assignment.rs) stores a target,
previous active generation, reason/evidence references and preparation/activation/
replacement/failure/uncertain outcome. Never overwrite an old target. Resolve the
current target using session.active_generation, not the newest assignment. Reserve
switches by conditional version update and use the reservation revision as a
never-reused generation, including after failed attempts. `pending_assignment_id`
fences dependent writes to the exact reservation.

Prepare/rebuild/resume resources on the replacement as supported by the concrete
API. Only after successful preparation may an atomic commit validate the expected
session version, activate the new assignment, replace the old one and advance the
active pointer/version. Stale workers/callbacks cannot overwrite a newer selection.
New work waits while switching; existing calls retain their original assignment.
An uncertain upstream create outcome must be recovered through lookup or supported
idempotency, not blindly replayed after a timeout. Store implements reserve,
activate and fail transitions; remote preparation and recovery stay in core.

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

This models v3 affinity/resource ownership and locally surveyed client APIs. Store
implements concurrent handoff persistence; exhaustion detection, resource recreation,
token translation and live WS continuation remain core/adapter work. An inventoried
endpoint alone is not evidence of cross-account migration support.
