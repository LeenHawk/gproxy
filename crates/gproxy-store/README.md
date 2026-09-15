# gproxy-store

English | [简体中文](README.zh-CN.md)

SeaORM 2 entity definitions for GPROXY v4. **This is an entity review draft**, not
an implemented repository or an applied database migration.

Start at [`src/entity/mod.rs`](src/entity/mod.rs). Entities are grouped by domain,
with one entity per file:

| Directory | Entities |
|---|---|
| `upstream` | Provider, Credential, Model, ProviderModel, OperationRule |
| `routing` | Namespace, Route, RouteTarget |
| `identity` | Organization, Team, OrganizationMember, TeamMember, User, ApiKey, UserSession, Permission |
| `limits` | RateLimit, Quota, QuotaWindow, QuotaSettlement, CredentialQuotaCycle |
| `pricing` | PriceRule, PriceRate, PriceTier |
| `usage` | UsageRecord, UpstreamCall, CaptureRecord |
| `resource` | FileObject, ResourceBinding, ProtocolState |
| `config` | Setting |

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
- Dedicated proxy columns resolve in order: `Credential.proxy`, `Provider.proxy`,
  then `Setting.proxy`. `None` inherits the next level. When all three are unset,
  global `inherit_system_proxy` controls system proxy fallback; otherwise connect
  directly. These are storage fields and an inheritance contract, not a wired
  network implementation.
- A team belongs to one organization. OrganizationMember and TeamMember store
  scoped `member`/`admin` roles, so one user's role can differ between groups.
- Organization credentials can be used by its members and descendant team users;
  team credentials can be used by team members. Viewing, editing and deleting
  shared credentials require the administrator role of their owning scope.
  Entity definitions record these relationships; API authorization is not implemented here.
- Provider models may reference a global model; deleting catalog metadata clears
  that optional reference.
- Route targets reference providers and optionally override the upstream model
  name. Routing does not require a model-catalog foreign key.
- Permission, rate-limit, and quota owner columns currently allow a user or an
  API key. The ownership shape is explicitly left for review before policy CRUD.
- Configuration-owned rows use the declared delete actions. Historical identity
  references have no configuration foreign keys. Quota settlements refer to quota
  windows, independently of removable usage detail records.
- File content stays in filesystem/S3 storage. File entities contain locators
  and metadata. Custom vocabulary configuration references a file object.
- Amounts use `Decimal(28,12)` as a proposed business representation. The current
  D1 adapter does not implement that mapping; decimal storage and precision must
  be settled before these entities are used for D1 data operations.
- OAuth issuer tables, reusable mutation rule sets, audit events, and derived
  usage rollups are outside this first entity draft.

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
