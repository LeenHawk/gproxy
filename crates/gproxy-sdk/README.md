# gproxy-sdk

English | [简体中文](README.zh-CN.md)

The embeddable GPROXY handle. `gproxy-core` executes a request against
providers it is handed; it never writes configuration, never resolves a model
name and never learns that a peer changed something. This crate is the layer
that does: it assembles a `Core` out of default implementations, owns the
configuration writes that advance `settings.config_revision`, turns a login
into a credential row, resolves a model name to an execution plan, and keeps
every instance of a deployment on the same revision through the shared cache
and a durable revision poll. The design notes behind those decisions are in
[`design/sdk.md`](../../design/sdk.md).

```rust
use gproxy_sdk::{GproxyBuilder, SyncMode};

# async fn example() -> Result<(), gproxy_sdk::SdkError> {
let gproxy = GproxyBuilder::sqlite("gproxy.db")
    .await?
    .master_key([0u8; 32])
    .sync_mode(SyncMode::Background)
    .build()
    .await?;

for channel in gproxy.channels() {
    println!("{} ({})", channel.display_name, channel.id);
}
println!("serving revision {}", gproxy.revision().0);
# Ok(())
# }
```

`build()` opens nothing it was not given: it synchronizes the entity schema
(unless told not to), creates the global settings row, assembles the engine,
loads the first snapshot and — in `SyncMode::Background` — starts the
subscription and poll loops. A secret codec is never chosen implicitly:
`master_key` seals with AES-256-GCM, `plaintext_secrets` is the explicit
opt-out, and a build with neither is refused.

## Features

| Feature | Effect |
|---|---|
| One channel's name (`codex`, `kiro`, `openai`, …) | Compile that channel in and register it by default |
| `channels` | Every channel this workspace ships |
| `postgres` / `mysql` | Extra SeaORM drivers, native only. SQLite is always available natively |
| `libsql` | libSQL/Turso over the Hrana HTTP pipeline, every target |
| `d1` | Marker for a Cloudflare D1 binding, which wasm32 always has |
| `memory` (default) | Process-local `MemoryCache`, the default cache on native targets |
| `redis` | Redis/Valkey, for a deployment with more than one instance |
| `fs` (default) | Local filesystem object storage, native only |
| `s3` | S3/R2 object storage |
| `bundled-vocabulary` (default) | Ship DeepSeek's vocabulary for token estimation |
| `ts` | `ts-rs` declarations for every DTO and for the channel descriptors, plus the export test — see [Type export](#type-export) |

The default set compiles for `wasm32-unknown-unknown`, and so does `libsql`.
On wasm the cache defaults to `gproxy_store::StoreCache` and synchronization is
always manual: an isolate does not outlive its request, so `tick()` at the top
of a request is the whole mechanism.

Native builds use the `reqwest` transport. A host that wants another backend
(`wreq` for TLS emulation, `reqwest-native` for the Codex CLI's stack) enables
that feature on `gproxy-client` itself, or hands the builder a `ClientPool` of
its own.

## Synchronization

Two mechanisms, because neither alone is enough.

| | Carries | Fails by |
|---|---|---|
| `Invalidation` on the shared cache | "look again", within milliseconds | Losing messages: a failed publish, a lagging subscriber, a closed topic |
| `settings.config_revision` poll | The durable truth, every 30s by default | Being slow |

A notification never carries state: it says which revision exists, and an
instance reloads only when that is ahead of the one it serves. An unreadable
payload, or one about a credential this snapshot has never seen, reloads rather
than guesses. Reloads are serialized and monotonic — an older revision never
replaces a newer one, and a failed reload leaves the previous snapshot serving.

## Calling

`call` builds the request, `send` resolves the model name and walks the plan.
A scope is required — it is the isolation boundary core keeps one caller's
credential affinity inside, and there is no safe default for it.

```rust
use gproxy_sdk::{Gproxy, SdkError};
use gproxy_core::{BudgetOwner, UsageAttribution};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest};

# async fn example<C>(gproxy: &Gproxy<C>, request: WireRequest<HttpBody>) -> Result<(), SdkError>
# where C: gproxy_seaorm::BatchConnectionTrait + Send + Sync + 'static {
let execution = gproxy
    .call(
        OperationKey { operation: Operation::GenerateContent, dialect: Dialect::OpenAi },
        request,
    )
    .scope("user:u-1")
    .attribution(UsageAttribution { user_id: Some("u-1".into()), ..Default::default() })
    .budgets(vec![BudgetOwner::new("user", "u-1")])
    .credentials(["cred-1".to_owned()].into())
    .send()
    .await?;

let (response, usage) = execution.into_parts();
# let _ = (response, usage);
# Ok(())
# }
```

`connect` is the same builder over a websocket handshake. Everything the
application layer decided — the allowed providers and credentials, the budget
chain, the session — is passed in; nothing here authenticates anyone.

For Responses, `connect().send()` uses the managed session driver: creates and
steering successors are admitted separately, including HTTP/SSE bridging, and
usage is recorded per turn. Hosts that admit turns themselves use
`open_responses()` and `begin_responses_turn()` instead.

## Resolution

A model name is resolved by the first rule that matches:

| Name | Resolves to | Attempt budget |
|---|---|---|
| absent | every enabled provider, no upstream model | `settings.max_attempts` |
| a model route name | that route's enabled members | the route's own |
| `channel/model` | the providers of that channel, preferring the ones whose catalog lists `model` | `settings.max_attempts` |
| `provider/model` | that one provider | `settings.max_attempts` |
| anything else | `SdkError::UnknownModel` | — |

Route names are matched exactly and before the prefix forms, so an operator
can expose the literal name `openai/gpt-5`. Within the prefix forms **a channel
id beats a provider of the same name**: a channel id is fixed by the build and
cannot be renamed out of the way, while a provider always can.

Candidates are then narrowed by channel, by allowed provider ids and by allowed
credential ids — the last is how an application layer keeps one organization's
credentials out of another's requests. Disabled, retired and `Dead` credentials
are dropped outright. A blocked credential is dropped while the provider still
has an unblocked one; a provider whose credentials are *all* blocked keeps them
and is ordered behind every healthy provider, so a rate limit is a last resort
rather than an outage. A name that resolved but reaches nothing is
`SdkError::NoTarget`, which is a different problem from an unknown name.

The order is `(tier, health, descending weight, stable id)`. Tier is a hard
preference; only the leading run — the candidates sharing the first one's tier
and health — is balanced, by the route's strategy: `RoundRobin` rotates it on a
per-route counter, `Weighted` promotes the smooth weighted pick, `Failover`
leaves it alone.

## Failover

Core already retries within one provider's credentials. `send` is the other
axis, and moves to the next provider only for failures another provider could
serve:

| Outcome | Next provider |
|---|---|
| `NoUsableCredential`, `CredentialDead`, `RefreshContended` | yes |
| `ContinuationElsewhere` | yes |
| any `Channel(..)` error, including transport failures | yes |
| a 401, 403, 429 or 5xx answer, or a refused websocket upgrade | yes |
| `BudgetExhausted`, `Forbidden`, `Cancelled`, `DeadlineExceeded` | no |
| `Transform`, `Route`, `Rewrite`, `OperationMismatch`, `InvalidTarget`, `NotImplemented` | no |
| `Store`, `Cache`, `Secret`, `Limits`, `Assembly`, `File` | no |

The attempt budget is shared: each target is granted at most as many attempts
as it has credentials and never more than what is left, so a plan cannot cost
more upstream calls than its `max_attempts`. With no target left, the last
answer is returned as it is — a 429 from the final provider is the caller's
429, not a synthesized error. The request body is buffered once so it can be
replayed; a streaming body past `max_request_body_bytes` stays a stream and the
plan is cut to a single target.

## Sessions

`session::extract` reads the caller's conversation identity, which is what
later keeps a conversation on one credential. It is read from the client that
actually sent the request, never from the upstream it is about to be forwarded
to. The ladder: the `x-gproxy-session-id` gateway header (or an explicit
`session_id()`), then `thread-id`, `session-id`, `x-claude-code-session-id`,
`x-conversation-id`, `x-grok-session-id`, then the native body field of the
inbound shape — Responses' `client_metadata.thread_id`/`session_id`, Claude's
`session_id` *inside* the JSON-encoded `metadata.user_id`, Gemini's
`request.session_id` and `request.sessionId` — then a sha256 fingerprint of the
conversation's stable prefix, and finally the request id, honestly labelled
`SessionSource::RequestFallback`.

Nothing recurses through arbitrary JSON looking for a field named `session_id`,
and request, turn and cache identifiers (`x-grok-req-id`, `user_prompt_id`,
`prompt_cache_key`, `previous_response_id`) are never sessions. The gateway
header is stripped before the request goes upstream. The full rules, the
evidence behind each field and the negative list are in
[`design/session-identity.md`](../../design/session-identity.md).

## Credential login

`gproxy.login()` turns a person's browser session into a credential row. Three
flows, whichever ones the provider's channel implements — `Gproxy::channels()`
reports each channel's `login_modes`, and asking for one a channel does not
offer is `SdkError::Unsupported`.

| Flow | Steps | For |
|---|---|---|
| Authorization code | `authcode_start` → `authcode_complete` | A browser redirect with PKCE |
| Device code | `device_start` → `device_poll`, repeatedly | A code typed on another device |
| Cookie exchange | `cookie_exchange` | A session cookie the person already has |

The SDK owns everything that is not the upstream's business. **PKCE**: the
verifier is 32 random bytes minted here, never sent, and only its S256 digest
reaches the authorize URL — an intercepted authorization code is useless
without it. **CSRF state**: minted here and compared here. `authcode_complete`
takes either the whole `callbackUrl` or a bare `code`, never both, and a state
that does not match the one the session was started with is refused *and*
destroys the session, so a replay cannot be retried into success.

The pending session lives in the shared cache under `gproxy-sdk:v1:login:{id}`
with its own TTL, never in this process. That is what lets any instance of a
deployment finish a login another one started — behind a load balancer it
usually is another one — and what makes an abandoned login cost nothing: the key
expires, and an expired key is `SdkError::LoginExpired`, exactly like an id that
was never issued.

**Polling is the caller's job.** `device_poll` performs one step and returns;
nothing here sleeps or loops. `Pending` carries the interval to wait, and an
upstream `slow_down` raises that interval for every later poll. `Denied` and
`Expired` are final and drop the session.

```rust
use gproxy_sdk::{Gproxy, SdkError, dto::{AuthCodeComplete, AuthCodeStart}};

# async fn example<C>(gproxy: &Gproxy<C>, callback_url: String) -> Result<(), SdkError>
# where C: gproxy_seaorm::BatchConnectionTrait + Send + Sync + 'static {
let started = gproxy
    .login()
    .authcode_start(AuthCodeStart { provider_id: "p-1".into(), ..Default::default() })
    .await?;
// send the person to `started.authorize_url`, then, when they come back:
let created = gproxy
    .login()
    .authcode_complete(AuthCodeComplete {
        login_session_id: started.login_session_id,
        callback_url: Some(callback_url),
        ..Default::default()
    })
    .await?;
println!("credential {}", created.credential_id);
# Ok(())
# }
```

A successful login is one ordinary credential insert through the same commit
primitive a management write uses: one revision, one reload, one notification.
The secret is sealed with the configured codec **before** the statement is
built, so nothing between the channel and the database sees it in the clear and
nothing sends it back — the answer is the credential's id and nothing else.
`auth_kind` is `oauth` for the two OAuth flows and `cookie` for the cookie
exchange, the owner columns are copied through untouched, and a caller that
supplied no label gets one derived from the channel and whatever account the
upstream named, made unique among that provider's labels.

## Management

`gproxy.manage()` is the write side: one accessor per configuration family, all
of them over the same primitive.

| Family | Rows | Beyond CRUD |
|---|---|---|
| `providers()` | `providers` | `reset_routing_defaults(provider_id)` drops the provider's operation rules and URLs in one commit |
| `credentials()` | `credentials` | `reveal_secret`, `set_status`, `refresh`, `quota_probe`, `quota_read`, `quota_reset_credits`, `quota_reset`, `quota_reset_with`, `health_reset`, `limit_status` |
| `models()` / `provider_models()` | `models`, `provider_models` | |
| `routes()` / `route_members()` | `routes`, `route_members` | |
| `connection_profiles()` | `connection_profiles` | |
| `settings()` | the single `settings` row | `get` / `update` only; split into an instance group and a logging group |
| `rewrite()` | `rewrite_rule_sets`, `rewrite_rules`, `provider_rewrite_rule_sets` | `replace_rules(set_id, rules)` replaces a whole set |
| `endpoints()` | `operation_rules`, `operation_endpoints` | |
| `quotas()` | `quotas` | `budget_status`, `reset_budget`, `limit_status`, `reset_limit` |
| `pricing()` | `price_rules`, `price_rates`, `price_tiers` | |
| `transfer()` | every configured row | `export`, `import` |
| `catalog()` | nothing, until applied | `channels`, `default_models`, `apply_default_prices`, `tls_presets`, `rule_presets`, `apply_rule_preset` |
| `connectivity()` | `provider_models` | `test`, `model_test`, `discover_models`, `apply_discovered` |
| `tokenizer()` | `file_objects`, `models.vocabulary_file_id`, `settings` | `vocabularies`, `fetch`, `progress`, `delete`, `auth`, `set_auth`, `reveal_auth` |

The first ten rows are ordinary CRUD over their tables:
`list(ListQuery) -> Page<Dto>`, `get(id)`, `create(Write)`, `update(id, Patch)`,
`delete(id)` and `batch(Vec<BatchItem>)` — with `settings()` the one exception,
a single row that is only read and patched. The last four are described under
[Operations catalogue](#operations-catalogue). Ids are
caller-supplied when given and minted otherwise, timestamps are Unix
milliseconds, decimals travel as strings, and `credentials.secret` is never in a
DTO — only `hasSecret`, plus the separate `reveal_secret` call.

`ListQuery`'s `(ownerKind, ownerId)` pair narrows the two families whose rows
carry an owner: `quotas` by its own two columns, `credentials` by the three
opaque ones (`user` / `team` / `org`, plus `instance` for the rows with no
owner at all). What those columns *mean* is the host's business — this crate
passes them through — but a list that could not be narrowed by them would force
a multi-tenant host to filter pages after the fact and report the unfiltered
counts, which is where a tenant boundary leaks. An owner kind the table cannot
hold matches nothing rather than everything.

### One write, one revision

```
commit_revision([statements…, bump, read])   one transaction
      ↓
reload   full, or `reload_credentials` when the write only touched credential state
      ↓
publish  Invalidation::ConfigurationChanged { revision, scopes }
```

The rows and the `config_revision` bump are the same transaction, so a write
that lands is never invisible to peers and a bump never happens without one. A
`batch` is one such transaction however many rows it names, and a refused write
never reaches the database at all: validation runs first, so the revision does
not move.

The reload happens before the notification, never after — a peer must not be
told about a revision this instance cannot serve yet. The notification itself is
best effort: a cache that refuses it costs the deployment one revision-poll
interval and is logged, not returned as an error.

A write names its `Scope`s, and that is the only thing the caller chooses.
`Scope::CredentialState` takes the cheap `reload_credentials` path, and it is
valid **only** for a write limited to the secret, its expiry and the lifecycle
status: everything else about a credential — label, auth kind, metadata,
connection profile, owner — is frozen into `CredentialData` at assembly and
needs a full reload. `Credentials::update` picks the right one from the patch.

### Reserved model-route prefixes

A model route name is matched exactly, but a name with a `/` in it is not
free: resolution reads the first segment of an unknown name as a narrowing
prefix. `codex/gpt-5` means "that model on the `codex` channel" and
`my-openai/gpt-5` means "that model on the `my-openai` provider". An exposed
name whose first segment is a registered channel id or an existing provider name
would therefore never be reached, so it is refused at write time rather than
left to fail silently. Anything else is fine: `coding/fast` is a perfectly good
public name.

## Operations catalogue

Four families that are not CRUD over one table: moving a whole configuration,
the data this build already holds, the two probes that leave the process, and
the vocabulary files local token estimation reads.

### Export and import

`manage().transfer()` writes the instance out as one document and reads one
back. The document is the management DTOs themselves, so an export says exactly
what a console would have listed.

```json
{
  "formatVersion": 5,
  "exportedAtMs": 1758412800000,
  "secretsOmitted": false,
  "secrets": ["aes-gcm"],
  "data": {
    "connectionProfiles": [], "providers": [], "credentials": [],
    "models": [], "providerModels": [],
    "routes": [], "routeMembers": [],
    "operationRules": [], "operationEndpoints": [],
    "rewriteRuleSets": [], "rewriteRules": [], "providerRewriteRuleSets": [],
    "quotas": [], "priceRules": [], "priceRates": [], "priceTiers": [],
    "settings": null
  }
}
```

`data` is in replay order: no row appears before the row it points at. A
credential is its own columns plus `secret`, which is
`{ "codec": "aes-gcm" | "plaintext", "bytes": "<base64>" }` — the database
column verbatim, never opened and never plaintext. With
`include_secrets: false` every `secret` is `null`, `secretsOmitted` is true and
`secrets` is empty.

**What does not travel.** Identity (users, API keys, organizations, teams,
permissions, subscriptions, OAuth clients) belongs to the application layer and
is out of this crate's reach. Usage records, quota windows, counted windows,
settlements, credential cycles and blocks, captures, agent sessions, protocol
states, cache rows and file objects are observations and runtime state: copying
them would fabricate history the destination never had. A `models`
`vocabularyFileId` or a `settings` `defaultVocabularyFileId` that names a file
the destination does not have is therefore cleared, with a warning, rather than
refused — and the vocabulary is re-fetched there.

`import` is one revision commit for the whole document, so a document that is
refused anywhere leaves nothing behind. `Merge` upserts by id and leaves
everything it does not mention alone; `Replace` additionally deletes the rows of
the exported kinds that the document omits, children before parents, and never
touches an identity, usage or capture table. A reference that resolves neither
in the document nor here is refused by name before anything is written.

**The master-key rule.** A sealed blob only opens under the key that sealed it,
and core opens every credential's secret while it assembles a snapshot — so a
credential imported unopenable would not break its own calls, it would break
every later reload of the whole instance. Hence:

| The importer has | What happens to a credential |
|---|---|
| `sourceMasterKey` (the source's 32 bytes, standard base64) | opened once and resealed under this instance's codec; counted in `credentialsResealed` |
| the same codec as the source, no key | stored verbatim, with a warning |
| neither | skipped with its row, counted in `credentialsSkipped` and warned |

A configuration-only export therefore creates no new credentials at all; it
updates the ones the destination already has and reports the rest as skipped.
`ImportReportDto` carries `created`, `updated`, `skipped`,
`credentialsResealed`, `credentialsSkipped` and the `warnings` — the warnings
are where a silently-cleared owner or vocabulary reference is reported.

### The static catalogues

`manage().catalog()` answers from this binary, not from the database.

- `channels()` — every compiled-in channel as a `ChannelDescriptor`: login
  modes, capabilities and the configuration keys a provider form should render.
  The same list as `Gproxy::channels()`.
- `default_models()` — the bundled model catalog: names, context windows and
  default prices for the models this release knew about. It is a snapshot taken
  when the asset was generated, not a live directory.
- `apply_default_prices({ providerId, modelIds, overwrite })` — writes catalog
  prices as `price_rules` with their rates and tiers, in one commit. Without a
  provider the catalog's own `*fragment*` glob and priority are used, so one
  rule prices that model wherever it is served; with a provider the literal
  name becomes the pattern at priority zero. `overwrite: false` is what makes
  re-applying the catalog safe — a rule an operator edited keeps its edit and
  is reported as `skipped`.
- `tls_presets()` — six client identities as `gproxy_client::EmulationConfig`
  objects, ready to store as a connection profile's `emulation`. Only the
  `wreq` backend presents one.
- `rule_presets()` / `apply_rule_preset({ ruleSetId, presetId })` — rewrite
  rule sets that make one client application look like a generic one. Applying
  a preset *replaces* the set's rules rather than merging: a preset is one
  ordered answer, and half of it interleaved with something else rewrites text
  nobody predicted. A caller that wants to keep existing rules reads
  `rule_presets()` and sends its own list to `rewrite().replace_rules`.

### The two probes

`manage().connectivity()` is the only part of this crate that leaves the
process.

`test({ scope })` asks Cloudflare's trace endpoint what this deployment looks
like from outside, through the client chain the scope names — `Global` (the
instance default profile), `Provider`, `Credential` (the very transport its
calls use), or `Proxy { url }` for a proxy that is not configured anywhere yet.
It answers `{ ok, latencyMs, ip, colo, error }`. **A network failure is
`ok: false` with a reason, not an `Err`**: "the upstream is unreachable" is the
answer that was asked for. An `Err` means the request was wrong — an unknown
provider, a credential that is not this provider's.

`model_test({ providerId, model, credentialId })` and
`discover_models(provider_id, credential_id)` go through `Core` exactly as a
caller's request would. **They spend a real credential, consume real upstream
quota, settle against any budget that credential is subject to, and write a
usage row** attributed to the scope `gproxy-sdk:admin` and the API key id
`admin-probe`. There is no dry run; a test that did not really call the
upstream would not have tested anything. One attempt, a thirty-second deadline
and no budgets of its own.

`discover_models` asks in the provider's own dialect, so nothing is converted
and the names are the upstream's; each one comes back with `known` (this
provider already has a `provider_models` row) and `hasDefaultPrice` (the
bundled catalog can price it). `apply_discovered(provider_id, names)` inserts
the rows, skipping the ones already there, so applying a discovery twice is the
same as once.

### Tokenizer vocabularies

Core estimates tokens for exchanges whose upstream reported none, and it does
that against a vocabulary file. `manage().tokenizer()` fetches them.

`fetch({ repo, filename, modelId, setAsDefault })` downloads
`https://huggingface.co/{repo}/resolve/main/{filename}` (default
`tokenizer.json`), sending the stored source token as
`Authorization: Bearer`, writes the bytes through the configured file storage,
and then inserts the `file_objects` row and points `models.vocabulary_file_id`
and `settings.default_vocabulary_file_id` at it — the row and both pointers in
one revision commit. The bytes land before the row: a row pointing at an object
that was never written would fail every reload, while an object with no row is
only wasted space. A non-2xx answer is refused with the upstream's own status,
because `404` and `401` mean very different things to whoever typed the
repository name, and the instance's `maxResponseBodyBytes` is the size cap.
Without file storage the whole family answers `Unsupported`: there is nowhere
to put the bytes.

`progress()` reports the download running **in this process** — one cell, not
per handle and not shared between peers, so a console asks for the fetch on one
request and polls progress on another. `delete(file_id)` drops the row inside a
commit that also releases whatever selected it, then removes the stored object.

`auth()` says only whether a source token is configured; `set_auth(token)`
seals it into the settings row exactly like a credential secret, under a fixed
identity, so a copy of the database carries no usable token; `reveal_auth()` is
the one deliberate disclosure, separate for the same reason revealing a
credential secret is separate from listing credentials.

## Queries

`gproxy.query()` is the read side of the same rows: three families over what
the engine left behind. Nothing here writes, so nothing here moves the
revision.

| Family | Answers |
|---|---|
| `usage()` | `records(UsageRecordQuery)`, `summary(UsageQuery)`, `group(UsageGroupQuery)`, `trend(UsageTrendQuery)` |
| `quota()` | `windows`, `settlements(window_id)`, `credential_cycles(credential_id)`, `counted_windows(credential_id, now)`, `budget_status(owners, now)` |
| `logs()` | `list(LogQuery)`, `detail(request_id)` |

Usage records and quota windows use the offset `Page<T>` the management
families use; request logs use a cursor. That is not a style choice: a
management list is a bounded set a person pages through, while a request log is
an append-only stream whose head keeps moving as it is read, and an offset page
over that repeats or skips rows.

Usage includes only operations that can produce inference or metered tool usage.
Model catalog reads, token counting, resource management and signaling-only calls
are excluded from both new usage rows and historical query results; they remain
in request logs. Summaries retain cache hits and separate cache writes for 5-minute,
30-minute and 1-hour retention, alongside the total cache-write count. The console
calculates cache hit rate as hits / (uncached input + hits + all cache writes).

### Aggregation happens in Rust, over a scan cap

`usage_records` stores one row per metered physical upstream call, without a
downstream summary row. Shared callers reference the call through capture links.
Token counts (including separate 5m/30m/1h cache writes), built-in media/tool
quantities, identities and USD cost have dedicated columns. Dynamic metrics and
pricing dimensions stay in the extension JSON. No log table is required.
Provider and credential filters run in SQL before pagination or the scan cap.
`summary`, `group` and `trend` currently fold matching rows in Rust, in key-ordered
chunks; this also includes arbitrary custom metric keys. `quantities` exposes
media/tool and custom quantities as exact decimal strings.

That is bounded. Every aggregate takes `maxScanRows`, defaulting to and clamped
by `query::MAX_SCAN_ROWS` (50 000), and an aggregation that reached its budget
comes back with `truncated: true` and the `scanned` count rather than a smaller
number presented as the whole truth. `trend` is bounded a second way: a zero or
negative `bucketMs`, a backwards range, and a range that would produce more than
`query::MAX_TREND_BUCKETS` (5 000) buckets are all refused outright.

Costs come from the dedicated column on each upstream usage row.
All prices use USD; `currency` is `None` only when nothing in the result was priced.

Grouping by `provider` or `credential` uses the corresponding usage columns.
Every matching physical call contributes once, including when several downstream
requests link to it. `requests` counts physical upstream calls; retries count
separately. A row with missing provider metadata lands under the empty key.

A token count on one record is `Option<u64>` — an upstream that did not report
a field did not measure zero — while every total is a plain `u64`, because a
sum of "nothing reported" really is zero.

### The log cursor

`logs().list` returns `nextCursor` and `nextCursorId`; handing both back
unchanged is the next page. Both halves are needed. The cursor is ordered on
`(started_at_ms, id)` descending, and two requests can start in the same
millisecond: a timestamp-only cursor either repeats that pair forever or skips
past it. `nextCursor` is `null` at the end of the list, and that is a fact —
the query reads one row past the page rather than guessing from a full one.

The downstream list reads `downstream_records`; the upstream list reads
`upstream_records`. `detail(request_id)` follows `capture_links` to resolve every
associated upstream call and returns its usage rows as a list. Shared upstream
calls have one physical record and one usage row regardless of link count.
Either side remains readable without the other side's log.

Stored bodies and individual event payloads are returned in full. The event
list is limited to `query::MAX_DETAIL_EVENTS` (2 000), with `eventsTruncated`
reporting that separate list limit. Every body travels
as a `LogBodyDto` carrying the record's own `body_state`, because a body that
was never captured must not read as a body that was empty — `notCaptured` with
zero bytes and `complete` with zero bytes are different facts. Text stays text;
anything else comes back base64.

### Redaction happened at write time

Nothing in `logs()` redacts. Core's observer applied the deployment's logging
redaction policy as it wrote these rows — headers, query parameters and secret
fragments in the stream — so what is stored is already what may be shown. A
host must not assume a second pass happens on read: if a secret is in the
database, it is because the policy allowed it there, and filtering on read
could not undo that.

### What the read side does not duplicate

Live status stays where it is computed. `manage().quotas()` owns
`budget_status`, `reset_budget`, `limit_status` and `reset_limit`;
`query().quota().budget_status` is the same core call with the clock passed in,
and `counted_windows` reads the raw meter rows for the dimensions core has no
status for — for a `limit:{quota_id}` dimension, `limit_status` is the
authoritative answer, because it knows the quota row and reports a normalized
decimal instead of the fixed-point atoms a cost meter counts in.

## Type export

The `ts` feature derives a `ts-rs` declaration for every type `dto` exports, and
one test writes them out:

```sh
GPROXY_TS_OUT=console/src/generated \
  cargo test -p gproxy-sdk --features ts export_types
```

Without `GPROXY_TS_OUT` the test returns immediately and writes nothing, so
`cargo test --all-features` stays hermetic and a generated directory is only
ever rewritten on purpose. With it, the directory is wiped first — a stale
declaration for a DTO that no longer exists would keep type-checking in the
console long after Rust dropped it — and an `index.ts` re-exporting everything
is written last.

The channel catalogue is in there as well: `dto` re-exports `ChannelDescriptor`
and its parts from `gproxy-channel`, and this crate's `ts` feature turns that
crate's on. A console renders a provider form from the descriptor the channel
itself returns, so it should be typed by that descriptor and not by a copy of
it that drifts the next time a channel adds a configuration key.

What the declarations say:

| Rust | TypeScript | Why |
|---|---|---|
| `i64` / `u64` | `number` | `with_large_int("number")`. Every timestamp and byte count here is far inside the range a JavaScript number holds exactly |
| a decimal amount | `string` | Money and limits are already decimal strings on the wire, for that same reason |
| `serde_json::Value` | `unknown`, or the shape the column is validated against | `paths` really is `string[]`, `corsOrigins` really is `string[]`; `config` and `metadata` are channel-defined and stay `unknown` |
| a tagged enum | the same tagged union | `ts(tag = …)` mirrors `serde(tag = …)`, field by field, so the union is the wire |
| `Option<T>` | `T \| null` | |

Two caveats worth knowing before writing against them. A patch field over a
nullable column is `Option<Option<T>>` and generates `T | null | null`, which
TypeScript reads as `T | null`; the third state — the key being absent, meaning
"leave the column alone" — is not expressible in a generated declaration, so a
caller builds a patch as `Partial<CredentialPatch>`, which is exactly the right
shape. And the two catalogue types that flatten a JSON object
(`DefaultModelDto`, `DefaultModelCatalogSourceDto`) come out as an intersection
with an index signature over `JsonValue`, generated into `serde_json/`.

The export list in `src/dto/export.rs` is hand-written, because Rust cannot
enumerate a module's types at run time. That list is what drifted in v3 — a DTO
was added, nobody remembered the list, and the console silently went without a
type for it — so a second test reads `src/dto/mod.rs` itself and compares its
`pub use` items against the list. Adding a DTO and forgetting the list is a red
test, not a missing file.

## What is not here

- **Identity.** Users, API keys, organizations, teams, permissions,
  subscriptions, rate limits and the OAuth issuer belong to the application
  layer, which reads the same durable revision through
  `gproxy_store::load_all_data`.
- **Downstream authentication.** Nothing here decides who a caller is; the
  handle is given a scope and a set of allowed providers and credentials.
- **A server.** No listener, no router, no middleware, no CLI. Native and edge
  hosts are built on top of this crate.
- **A console.** This crate generates the TypeScript types a UI is written
  against; the UI itself, its session handling and its HTTP transport are the
  application's.
- **Execution.** Attempts, conversion, rewriting, observation, budgets and
  settlement are `gproxy-core`'s; this crate only decides what to hand it.
