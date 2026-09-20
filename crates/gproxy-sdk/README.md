# gproxy-sdk

English | [简体中文](README.zh-CN.md)

The embeddable GPROXY handle. `gproxy-core` executes a request against
providers it is handed; it never writes configuration, never resolves a model
name and never learns that a peer changed something. This crate is the layer
that does: it assembles a `Core` out of default implementations, owns the
configuration writes that advance `settings.config_revision`, turns a login
into a credential row, resolves a model name to an execution plan, and keeps
every instance of a deployment on the same revision through the shared cache
and a durable revision poll.

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
| `custom` / `codex` / `claudecode` / `claudeweb` | Compile a channel in and register it by default |
| `postgres` / `mysql` | Extra SeaORM drivers, native only. SQLite is always available natively |
| `libsql` | libSQL/Turso over the Hrana HTTP pipeline, every target |
| `d1` | Marker for a Cloudflare D1 binding, which wasm32 always has |
| `memory` (default) | Process-local `MemoryCache`, the default cache on native targets |
| `redis` | Redis/Valkey, for a deployment with more than one instance |
| `fs` (default) | Local filesystem object storage, native only |
| `s3` | S3/R2 object storage |
| `bundled-vocabulary` (default) | Ship DeepSeek's vocabulary for token estimation |
| `ts` | `ts-rs` declarations for the DTOs |

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

## Resolution

A model name is resolved by the first rule that matches:

| Name | Resolves to | Attempt budget |
|---|---|---|
| absent | every enabled provider, no upstream model | `settings.max_attempts` |
| an exposed model name | that route's enabled members | the route's own |
| `channel/model` | the providers of that channel, preferring the ones whose catalog lists `model` | `settings.max_attempts` |
| `provider/model` | that one provider | `settings.max_attempts` |
| anything else | `SdkError::UnknownModel` | — |

Exposed names are matched exactly and before the prefix forms, so an operator
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
| `Transform`, `Route`, `Rewrite`, `OperationMismatch`, `InvalidTarget` | no |
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

## Management

`gproxy.manage()` is the write side: one accessor per configuration family, all
of them over the same primitive.

| Family | Rows | Beyond CRUD |
|---|---|---|
| `providers()` | `providers` | `reset_routing_defaults(provider_id)` drops the provider's operation rules and URLs in one commit |
| `credentials()` | `credentials` | `reveal_secret`, `set_status`, `refresh`, `quota_probe`, `quota_read`, `quota_reset`, `health_reset`, `limit_status` |
| `models()` / `provider_models()` | `models`, `provider_models` | |
| `routes()` / `route_members()` / `exposed_models()` | `routes`, `route_members`, `exposed_models` | |
| `connection_profiles()` | `connection_profiles` | |
| `settings()` | the single `settings` row | `get` / `update` only; split into an instance group and a logging group |
| `rewrite()` | `rewrite_rule_sets`, `rewrite_rules`, `provider_rewrite_rule_sets` | `replace_rules(set_id, rules)` replaces a whole set |
| `endpoints()` | `operation_rules`, `operation_endpoints` | |
| `quotas()` | `quotas` | `budget_status`, `reset_budget`, `limit_status`, `reset_limit` |
| `pricing()` | `price_rules`, `price_rates`, `price_tiers` | |

Every family has `list(ListQuery) -> Page<Dto>`, `get(id)`, `create(Write)`,
`update(id, Patch)`, `delete(id)` and `batch(Vec<BatchItem>)`. Ids are
caller-supplied when given and minted otherwise, timestamps are Unix
milliseconds, decimals travel as strings, and `credentials.secret` is never in a
DTO — only `hasSecret`, plus the separate `reveal_secret` call.

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

### Reserved exposed-model prefixes

An exposed model name is matched exactly, but a name with a `/` in it is not
free: resolution reads the first segment of an unknown name as a narrowing
prefix. `codex/gpt-5` means "that model on the `codex` channel" and
`my-openai/gpt-5` means "that model on the `my-openai` provider". An exposed
name whose first segment is a registered channel id or an existing provider name
would therefore never be reached, so it is refused at write time rather than
left to fail silently. Anything else is fine: `coding/fast` is a perfectly good
public name.

## What is not here

- **Identity.** Users, API keys, organizations, teams, permissions,
  subscriptions, rate limits and the OAuth issuer belong to the application
  layer, which reads the same durable revision through
  `gproxy_store::load_all_data`.
- **Downstream authentication.** Nothing here decides who a caller is; the
  handle is given a scope and a set of allowed providers and credentials.
- **A server.** No listener, no router, no middleware, no CLI. Native and edge
  hosts are built on top of this crate.
- **Execution.** Attempts, conversion, rewriting, observation, budgets and
  settlement are `gproxy-core`'s; this crate only decides what to hand it.
