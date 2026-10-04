# gproxy-channel

English | [简体中文](README.zh-CN.md)

Upstream adapters for GPROXY v4. A channel knows one upstream family: its
URLs, how a credential is injected, which wire dialects it speaks, how its
answer is shaped into the standard response of the operation (which is where
the host reads usage) and, for account-style upstreams, how to log in,
refresh and read quota. Everything else (routing, credential selection, failover,
protocol conversion, settlement, capture) belongs to `gproxy-core`; nothing
here reads the database or picks a transport.

The crate depends on `gproxy-protocol` for operations and wire types and on
`gproxy-client` only for the `OutboundClient` contract, which it re-exports.
No concrete channel is compiled by default; each is a Cargo feature:

| Feature | Id | Upstream | Credential |
|---|---|---|---|
| `aistudio` | `aistudio` | Google AI Studio: the native Gemini methods and the `/v1beta/openai` compatibility layer off one origin, SSE and JSON-array streams, `usageMetadata` metering | `{"api_key"}` |
| `antigravity` | `antigravity` | A Google account through the Code Assist host the Antigravity editor talks to: PKCE login that discovers the Cloud project and tier, refresh, the Code Assist request envelope, `fetchAvailableModels` catalogue and per-model quota | `OAuthCredential` |
| `aws_bedrock` | `aws_bedrock` | AWS Bedrock: SigV4-signed (or API-key) calls on `bedrock-runtime` with the wire chosen per model — `InvokeModel` for Anthropic models, event-stream replies translated into Claude Messages SSE, and the OpenAI-compatible Chat Completions for the rest — plus the control plane's foundation-model directory | AWS access key pair (optionally temporary) or a Bedrock API key |
| `azure` | `azure` | Azure OpenAI and the Anthropic models Azure AI Foundry hosts: the v1 or the deployment-scoped surface under a resource, `api-key` and `x-api-key` | `{"api_key"}` |
| `claudeapi` | `claudeapi` | Anthropic's own API: `x-api-key` Messages with the API's request hygiene and server-side fallback, the OpenAI SDK compatibility layer, `anthropic-ratelimit-*` headers, `/v1/organizations/cost_report` | `{"api_key", "quota_api_key"}` |
| `claudecode` | `claudecode` | Claude.ai subscription through the Claude Code CLI's Messages requests: PKCE and cookie login, refresh, unified rate-limit headers, `/api/oauth/usage`, CLI services | `OAuthCredential` |
| `claudeweb` | `claudeweb` | claude.ai browser session: cookie login against `/api/bootstrap`, multi-call conversation turns rendered as Claude Messages SSE, organization usage windows | session cookie + organization |
| `cline` | `cline` | Cline's own account at `api.cline.bot`: a WorkOS device login registered with Cline, a `workos:`-prefixed bearer, the SDK's attribution headers, the `{success, data}` envelope its replies arrive in, its recommended-model groups, plan windows and credit balance | `{"api_key"}` or `OAuthCredential` |
| `cloudflare_ai_gateway` | `cloudflare_ai_gateway` | Cloudflare AI Gateway through the Cloudflare REST API: `/client/v4/accounts/{account}/ai` with the account and `cf-aig-gateway-id` taken from the credential, one bearer on Chat, Responses and Claude Messages, the account's credit balance | `{"api_key", "account_id", "gateway_id"?}` |
| `codex` | `codex` | ChatGPT account through the Codex backend: OAuth (PKCE and device code), Responses over HTTP SSE and WebSocket, `x-codex-*` limit headers, `/wham/usage`, CLI backend services | `OAuthCredential` |
| `copilotcli` | `copilotcli` | GitHub Copilot through the `copilot` CLI: a GitHub device login whose long-lived token is exchanged by `CredentialRefresh` for the short-lived Copilot token, the CLI's editor identity, the seat's origin, `copilot_internal/user` quota snapshots | GitHub OAuth token (+ minted Copilot token) |
| `custom` | `custom` | Any API-key endpoint speaking OpenAI, Claude or Gemini natively | `{"api_key"}` |
| `dashscope` | `dashscope` | Alibaba DashScope: the OpenAI-compatible mode, the Anthropic-compatible mode, a rerank prefix of its own and the native multimodal-generation image API, all on one origin | `{"api_key"}` |
| `deepseek` | `deepseek` | DeepSeek: Chat Completions under `/v1`, Responses at the origin root, Claude Messages under `/anthropic`, `prompt_cache_hit_tokens`, `/user/balance` | `{"api_key"}` |
| `glm` | `glm` | GLM pay-as-you-go API; live official ZCode model catalogue | `{"api_key"}` |
| `glm` | `glmcode` | GLM Coding Plan; separate catalogue, Chat / Messages / Responses and subscription quota | `{"api_key"}` |
| `minimax` | `minimax` | MiniMax text API / Token Plan and H3 video creation, query, list, cancellation/deletion and download | `{"api_key"}` |
| `devin` | `devin` | Devin (Windsurf) at `server.codeium.com`: Connect-RPC over protobuf rather than JSON, `GetChatMessage` frames translated into Chat Completions SSE, `GetUserStatus` daily and weekly windows | session token |
| `geminicli` | `geminicli` | A Google account through the Code Assist endpoints the Gemini CLI talks to: PKCE login that discovers the Cloud project and tier, refresh, the Code Assist request envelope, `retrieveUserQuota` catalogue and per-model quota | `OAuthCredential` |
| `grokbuild` | `grokbuild` | An xAI account through the Grok Build CLI: device login at `auth.x.ai`, refresh, a narrowed Responses body on `cli-chat-proxy.grok.com`, xAI's own media paths on `api.x.ai`, `/billing?format=credits` | `OAuthCredential` |
| `kimi` | `kimi` | Moonshot: the platform at `api.moonshot.cn` with an API key, or the Kimi Code subscription at `api.kimi.com` through a device login, refresh and the CLI's `x-msh-*` identity; `/usages` windows or a cash balance | `{"api_key"}` or `OAuthCredential` |
| `kiro` | `kiro` | AWS CodeWhisperer through the Kiro desktop app: device or IAM Identity Center login, refresh, a Responses request turned into the CodeWhisperer conversation envelope, an AWS event-stream reply translated back into Responses SSE, `GetUsageLimits` credit windows | `OAuthCredential` |
| `nvidia` | `nvidia` | NVIDIA NIM: Chat Completions with the streamed-usage opt-in, the model list and embeddings | `{"api_key"}` |
| `openai` | `openai` | OpenAI's own platform: the full OpenAI surface, Responses and Realtime over a WebSocket, `x-ratelimit-*` headers, `/v1/organization/costs` | `{"api_key", "quota_api_key"}` |
| `opencode` | `opencodezen` | OpenCode Zen, pay-as-you-go from the Console balance: Chat Completions, Responses and Claude Messages, the CLI's `x-opencode-session` affinity header, a Console device login and refresh | `{"api_key"}` or `OAuthCredential` |
| `opencode` | `opencodego` | OpenCode Go, the subscription: the same three surfaces on its own origin, the affinity header and the `/usage` windows | `{"api_key"}` |
| `openrouter` | `openrouter` | OpenRouter: provider routing preferences filled into the body, `HTTP-Referer`/`X-Title` attribution, the price the reply says it charged, `/v1/auth/key` | `{"api_key"}` |
| `vercel` | `vercel` | Vercel AI Gateway: Chat, Responses, Claude Messages and embeddings, dual key headers, the team's `/v1/credits` balance | `{"api_key"}` |
| `vertex` | `vertex` | Google Vertex AI: regional project-scoped methods for the Google, Anthropic and OpenAI-compatible publishers; a service-account key exchanged for an access token through `CredentialRefresh` | Google service-account key |
| `vertexexpress` | `vertexexpress` | Vertex AI Express mode: the Gemini surface on one global origin, key in the query, no project and no region | `{"api_key"}` |
| `workbuddy` | `workbuddy` | Tencent Copilot through its editor plugin: device login, refresh, OpenAI Chat under `/v2` with the plugin's identity and account headers, the `/v3/config` catalogue, the enterprise and personal billing meters | `OAuthCredential` |
| `xai` | `xai` | xAI (Grok): OpenAI Chat and Responses plus xAI's own `/v1/tts`, `/v1/stt` and `/v1/videos/generations`, `cost_in_usd_ticks` metering, the management billing probe | `{"api_key"}` |

Every channel builds for native targets and `wasm32-unknown-unknown`.

`claudeapi`, `openai` and `aistudio` are the vendors' own first-party APIs.
Each exists as a channel rather than as a `custom` provider because of a
route layout `custom` cannot express: Anthropic's routes follow the operation
rather than the client's path, OpenAI serves Responses and Realtime over a
socket, and AI Studio puts two surfaces on one origin with a different
credential header on each.

`dashscope`, `deepseek`, `kimi`, `openrouter` and `xai` are the API-key fleet:
vendors whose wire is one of the three compatible shapes, so they share
`channels::shared::compatible` for bounded ability calls, and differ only in
where a method lives, which extra fields the usage object carries (each
channel's `UsageExtras`), and what an account surface reports. Each earns a
channel by having something `custom` cannot state — see *Vendors that need no
channel* below for the ones that do not.

Two of them report a price rather than only counts. `openrouter` records
`upstream_cost_usd` whenever usage accounting is on, and `xai` records it for
a video job, which states its own dollars; both also set the
`upstream_priced` dimension. Core prices from the store's rate rows and knows
nothing about these metrics by name, so an operator who wants the upstream's
own number to be the bill writes one rate row for `upstream_cost_usd` at 1
USD per unit and no token rows. xAI's `cost_in_usd_ticks` is recorded under
that name and is deliberately *not* dollars: it is xAI's own unit and needs a
rate row of its own.

`azure`, `vertex` and `vertexexpress` are cloud resellers: they forward each
hosted vendor's own wire and change only where the method lives and how the
credential is presented. None of them impersonates a client, so none returns a
`default_connection` — the operator's connection profile is the only thing
that decides their outbound stack. A `vertex` credential is stored with its
`expires_at_ms` already past, so the host's first refresh mints the access
token before the first request goes out; `prepare` never mints one.

`aws_bedrock` is the one channel that signs rather than presents a token:
every request carries an AWS SigV4 `Authorization`, computed in `prepare`
because signing is pure arithmetic over the request and the credential. Bedrock
takes a different wire per model, so the channel overrides
`native_dialects_for_model`: Anthropic models go to `InvokeModel`, whose
streaming reply is AWS event-stream framing that `stream_generate_content`
translates into Claude Messages SSE; every other model goes to the
OpenAI-compatible Chat Completions, SSE as it is. Families with neither
(Llama, Nova, …) need Bedrock's `Converse` shape, which `gproxy-protocol`
cannot express yet.

`geminicli` and `antigravity` are the Gemini family's CLI impersonation
channels: a Google account's OAuth credential used against the Code Assist
internal endpoints those two tools talk to, not a Gemini API key. They share
the Code Assist request envelope, the Google login and the one-time project
and tier discovery, and differ in their client id, scopes, user agent, host
and catalogue method.

`cline`, `copilotcli` and `opencode` are coding tools with accounts of their
own rather than vendors. Each fronts many models behind one wire it did not
invent, so what the channel exists for is everything around the request: a
device login that ends somewhere other than where it started, a credential
that is not what the inference endpoint accepts, a reply that arrives wrapped,
a catalogue that is not a model list, and an account surface on a different
host from the traffic. Only `copilotcli` has a client identity worth
reproducing (v3 recorded a `profile.rs` for it and for neither of the others),
so only it returns a `default_connection`.

`copilotcli` is the one channel whose credential cannot be used as it is
acquired. A GitHub device login yields a long-lived GitHub OAuth token; Chat
Completions accepts only a Copilot token minted from it, good for minutes.
That mint is `CredentialRefresh` — the GitHub token is the refresh token and
the Copilot token the access token — because `prepare` is synchronous and pure
and would otherwise re-mint on every request. The login's final step performs
the first mint, so the credential the host persists is usable at once.

`kiro`, `workbuddy` and `grokbuild` are three more CLI impersonation
channels, and share nothing but that shape: each is one vendor's coding tool,
its account login, and the identity headers that tool sends. `kiro` is the
furthest from any public API — CodeWhisperer takes a conversation envelope
over Smithy JSON and answers in AWS event-stream framing, so the channel
builds the envelope from a Responses body and translates the reply back into
Responses SSE. `workbuddy` and `grokbuild` speak OpenAI, and their work is
identity and narrowing: WorkBuddy's account block and gateway envelope,
Grok Build's two origins and the Responses fields its chat proxy refuses.
All three read the session id their client sent rather than minting one:
`design/session-identity.md` names WorkBuddy's `x-conversation-id` and Grok
Build's `x-grok-session-id`, and names the per-request and per-turn ids
beside them as explicitly not sessions. Kiro has no entry in that ladder and
its client sends none.

Grok Build Responses default to
`store: false` and request encrypted reasoning for replay. Requests with a
reasoning effort consult the account's model catalogue, cached in scoped
channel state for five minutes; an unavailable catalogue leaves the caller's
effort intact. Billing reads both `/billing?format=credits` and optional
`/settings`, exposing prepaid and on-demand USD amounts, historical usage,
subscription labels and shared-credit status. On-demand and history readings
are observations and do not independently block account selection.

## Vendors That Need No Channel

A channel is code to maintain against someone else's wire. A vendor earns one
only when a provider row cannot state what it needs: a path that moves with
the operation, a body that must be rewritten, an account surface to read, a
client identity to present. A vendor whose whole difference is an origin and
a header is a `custom` provider; `base_url` is the provider column and
everything else its `config` JSON:

```json
{ "base_url": "https://api.example-vendor.com", "config": { "dialects": ["openai_chat"] } }
```

NVIDIA NIM, Vercel AI Gateway and Cloudflare AI Gateway were served this way
for a while and each turned out to need a channel after all: NVIDIA's streamed
usage has to be requested in the body, Vercel and Cloudflare report a balance,
and Cloudflare keeps its account and gateway per credential and wants its
bearer on the Claude surface too.

## Contract

`BaseChannel` is the only required trait and `id` is its only required method.
Every protocol `Operation` has its own asynchronous method with a default
implementation: build the request with `prepare` (HTTP) or `prepare_connect`
(WebSocket), then hand it to the assigned client. Both preparation hooks
default to `UnsupportedOperation`, so a channel supports exactly what it
prepares or overrides, never more.

```text
host selects provider, credential, client
  → ChannelBinding::new(&channel, provider, credential, client)
  → binding.send(OperationKey, WireRequest<HttpBody>)
    binding.connect(OperationKey, WireRequest<()>)
  → the named operation method on BaseChannel
  → default: prepare + client.send / prepare_connect + client.connect
    or the channel's own multi-call flow through the same client
  → WireResponse<HttpBody> / UpstreamConnection
```

The views the host hands over are borrowed and public where they can be:

| Type | Holds |
|---|---|
| `ProviderView` | `id`, `channel`, optional `base_url`, JSON `config` the channel decodes into its own typed settings |
| `CredentialView` | `id`, `provider_id`, `auth_kind`, JSON `secret` (no `Debug`, no `Serialize`), public `metadata` the host recorded (plan, account id), `version`, `expires_at_ms` |
| `OperationContext` | the two views, the operation's `Dialect`, the `WireRequest`, an owned `Arc<dyn OutboundClient>`, `ChannelState`, the host `instance_id` and an optional `endpoint_override` |
| `CredentialContext` | provider, credential and client for refresh, quota and services |

Rules every channel follows:

- Preparation is synchronous and pure: no I/O, no hidden state, no client
  construction. Operation overrides may make several exchanges, but only
  through `context.client`, and may keep it to finish work after the
  response stream ends (releasing a vendor conversation, for instance).
- Source authentication never reaches the upstream. `forwardable` drops
  hop-by-hop headers, `host`, `content-length`, `authorization`, `x-api-key`,
  `x-goog-api-key` and `api-key`; the channel adds its own auth from the
  credential. Client headers are allowed by the union of the global
  `allowedHeaders`, the provider's `config.allowed_headers`, and the channel's
  declared `ChannelHeaders`; `content-type` always passes. Missing or empty
  lists add no entries, so unlisted client headers are dropped by default.
- `endpoint_override`, when set, is the complete method URL and replaces
  `base_url` plus the channel's default path.
- Responses keep status, headers and a lazy body; a non-2xx is a response,
  not an error. `ChannelError::UpstreamResponse` is for ability calls (login,
  refresh, quota) and for an intermediate exchange inside a multi-call
  override, where the failing reply is not the response being returned.
- `RefreshRejected` means the upstream definitively refused the credential
  (`invalid_grant`, revoked); the host marks it dead. Transient transport
  and 5xx failures must surface as `Transport` so the host retries later.
- `ContinuationElsewhere { instance_id }` says a live upstream connection is
  held by another host process; the host reroutes, nothing is wrong with the
  credential.

## Optional Abilities

Each ability is an independent trait returned by a default-`None` accessor on
`BaseChannel`. A channel implements the ones its upstream has and nothing
forces a coupling between them (a channel may report quota windows without
offering a reset).

| Accessor | Trait | Purpose |
|---|---|---|
| `credential_refresh` | `CredentialRefresh` | Produce a full replacement secret and expiry; the host persists it with a CAS on `version` |
| `oauth_authorization_code` | `OAuthAuthorizationCode` | `authorize` builds the URL for a host-supplied PKCE challenge and state; `exchange` turns the code into an `OAuthCredential` |
| `oauth_device_code` | `OAuthDeviceCode` | `start` and one `poll` step; scheduling belongs to the host |
| `cookie_login` | `CookieLogin` | Exchange a pasted cookie for an `AcquiredCredential` (secret plus public metadata) |
| `quota_model` | `QuotaModel` | Synchronous, pure: the `QuotaDimension`s a credential has, derived from `auth_kind` and metadata |
| `quota_query` | `QuotaQuery` | Read a `QuotaSnapshot` from the upstream's usage endpoint |
| `quota_headers` | `QuotaHeaders` | Turn response headers into `QuotaEntry`s; empty means nothing reported |
| `quota_reset` | `QuotaReset` | Credits and a manual reset on upstreams that sell them |
| `usage_extras` | `UsageExtras` | The vendor's own usage fields (a charged price, a cache counter under its own name, the model that served), read beside the standard usage the host reads from the channel's shaped response |
| `services` | `ChannelServices` | Vendor control-plane routes (Codex plugins, files, remote control; Claude Code files) with a `ServiceCaller` the host implements for identity, role, usage and resource bindings |

`ChannelState` is cross-request memory the host scopes to one provider and
one credential (a web conversation to resume, a session to reuse). Channels
write under keys they choose through compare-and-swap; `NoState` is the
binding default and refuses every write.

Both OAuth flows carry the same pocket for facts a channel needs on the far
side of the person's authorization: `AuthorizationStart::provider_state` comes
back as `AuthorizationCode::provider_state`, and
`DeviceAuthorization::provider_state` comes back with the authorization itself.
The shape is the channel's own — a non-standard device handle, a client a login
registered for itself. Unlike `OAuthCredential::provider_fields`, which the
host publishes as the credential's metadata, **this pocket may carry secrets**:
it lives in the host's login session, which is short-lived, cache-backed and
never rendered, and it is gone when the login ends. Anything that has to
outlive the login must be returned from the exchange — public facts in
`provider_fields`, secret ones in `OAuthCredential::provider_secrets`, which is
sealed with the tokens and never becomes metadata.

Codex, Claude Code, Cline, Antigravity, Devin, Kiro, Copilot CLI and Grok Build
support a per-credential `metadata.allow_paid_usage`
boolean, defaulting to `false`. The Console exposes it as **Allow credit
balance usage**. `QuotaModel::allows_paid_usage` identifies the subscription
windows it may bypass. Core retains their exhaustion records and applies the
current permission during selection, so switching it off enforces the known
limits again. Local budgets and actual upstream refusal cooldowns still apply.

This permission does not enable upstream billing or buy credits. Claude Code
requires extra usage to be enabled on the account already. Inspection of
Claude Code 2.1.285 found account-level `overage_spend_limit` settings, not a
per-request no-overage flag. The local switch cannot prevent charges on a
request already sent or one that first reveals an exhausted subscription.

Cline plan windows cover `cline-pass/` models. Zero credits do not block
subscription models or `cline-free/` and `:free` models, including when an older
version persisted an account-wide balance block. Cline also reads its recommended
catalog to exclude free models without a naming prefix. New catalog scopes
replace older balance scopes. Copilot scopes premium exhaustion using catalog
billing metadata; an unknown scope stays observe-only.
Kiro includes an active free-trial allowance before applying its monthly limit.
Kimi/Moonshot and OpenCode Go/Zen are separate products; this switch does not
move requests between them or change the requested model.

## Adding a Channel

Channels are built in, not plugged in: a new one is a module of this crate
behind its own feature. The steps, using `custom` (API key) and `codex`
(OAuth account) as the two references.

1. Declare the feature in `Cargo.toml`. List only the optional dependencies
   the channel needs; a wasm-only dependency goes under
   `[target.'cfg(target_arch = "wasm32")'.dependencies]` as `claudeweb`'s
   `send_wrapper` does.

   ```toml
   [features]
   # One-line description of the upstream and what the channel covers.
   acme = ["dep:base64", "dep:web-time"]
   ```

2. Add one line to the `channels!` list in `src/channels/mod.rs`: the feature,
   the module, and the value a host registers. That one line declares the
   module and puts the channel in `channels::compiled_in()`, which is how every
   host finds it. If the channel uses `shared`, extend the `cfg(any(...))` on
   that module too — it keeps its own list.

   ```rust
   "acme" => acme, acme::Acme;
   ```

3. Lay the module out one concern per file. Small channels are a single
   `acme.rs`; larger ones a directory:

   ```text
   src/channels/acme/
     mod.rs        ID, the unit struct, re-exports, module doc with wire facts
     config.rs     AcmeConfig: provider `config` JSON, serde(default), unknown keys ignored
     request.rs    impl BaseChannel: prepare / prepare_connect / overridden operations
     oauth.rs      OAuthAuthorizationCode, OAuthDeviceCode, CookieLogin, CredentialRefresh
     quota.rs      QuotaModel, QuotaQuery, QuotaHeaders
     usage.rs      UsageExtras, UsageSource
     services.rs   ChannelServices routes and handlers
   ```

   The module doc names the source of every wire fact (the vendor's API
   reference or CLI source under `samples/`). Wire truth is never another
   channel's code.

4. Implement `BaseChannel`. The minimum is `id`, `native_dialects` and
   `prepare`; the struct is a stateless unit so one instance serves every
   provider of that channel. Override `descriptor` as well: the default reads
   the capability accessors, and only the channel knows its display name and
   which keys it decodes out of the provider `config` JSON. That descriptor is
   all a management UI has to render a provider form from — under the `ts`
   feature it is a TypeScript type too — so a key left out of it is a key
   nobody can set.

   ```rust
   use gproxy_channel::channel::{
       BaseChannel, ChannelError, HeaderAllowlist, PrepareContext, ProviderView, forwardable,
   };
   use gproxy_protocol::{Dialect, HttpBody, Operation};
   use http::{HeaderValue, header};
   use serde::Deserialize;

   pub const ID: &str = "acme";

   #[derive(Debug, Default, Deserialize)]
   #[serde(default)]
   pub struct AcmeConfig {
       pub headers: std::collections::BTreeMap<String, String>,
   }

   #[derive(Debug, Default, Clone, Copy)]
   pub struct Acme;

   impl BaseChannel for Acme {
       fn id(&self) -> &'static str {
           ID
       }

       fn native_dialects(&self, _provider: ProviderView<'_>, _operation: Operation) -> Vec<Dialect> {
           vec![Dialect::OpenAiChat]
       }

       fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
           let config: AcmeConfig = serde_json::from_value(ctx.provider.config.clone())
               .map_err(|e| ChannelError::InvalidConfig(e.to_string()))?;
           let key = ctx.credential.secret["api_key"]
               .as_str()
               .filter(|k| !k.is_empty())
               .ok_or(ChannelError::InvalidCredential)?;
           let base = ctx.provider.base_url.unwrap_or("https://api.acme.example/v1");
           let uri = match ctx.endpoint_override {
               Some(url) => url.to_owned(),
               None => format!("{}{}", base.trim_end_matches('/'), ctx.request.path),
           };
           let allowlist = HeaderAllowlist::from_view(ctx.provider)?;
           let mut headers = forwardable(&ctx.request.headers, allowlist.as_ref(), &[]);
           headers.insert(
               header::AUTHORIZATION,
               HeaderValue::from_str(&format!("Bearer {key}"))
                   .map_err(|_| ChannelError::InvalidCredential)?,
           );
           let _ = config.headers; // static headers, query handling, body shaping go here
           let mut builder = http::Request::builder().method(ctx.request.method).uri(uri);
           *builder.headers_mut().expect("fresh builder") = headers;
           builder
               .body(ctx.request.body)
               .map_err(|e| ChannelError::InvalidConfig(e.to_string()))
       }
   }
   ```

   Override an operation method instead of `prepare` when the upstream needs
   more than one exchange, a locally synthesized reply (`list_models` from a
   static catalogue), a different transport, or a response rewritten into the
   declared native dialect. `codex` overrides the Responses operations to add
   CLI identity headers and turn-state replay; `claudeweb` overrides
   `generate_content` and `stream_generate_content` to drive a browser
   conversation through several calls.

5. Declare what the vendor client sends. If the channel impersonates a CLI,
   export a `ChannelHeaders` constant and build the allow-list with
   `HeaderAllowlist::from_view_for(provider, HEADERS)` so a provider's
   `allowed_headers` cannot strip the CLI's own headers. Pass the channel's
   identity headers in `forwardable`'s `also_drop` so a client cannot spoof
   them. Return a `default_connection()` when the upstream fingerprints its
   clients (`codex` uses reqwest over native TLS, `claudeweb` a browser
   preset); an explicit host profile on the credential or provider still
   wins.

6. Add abilities as separate types and return them from the accessors.
   Keep each trait's rules: refresh returns a full replacement, never a
   merge; `QuotaModel::dimensions` reads plan facts from
   `credential.metadata`, never the network; a channel reads no usage of its
   own — it shapes its answer into the standard response, and `UsageExtras`
   only adds what its vendor reports beside the standard usage object;
   `RefreshRejected` only for a definitive upstream refusal.
   Public facts a login discovers (plan, account id) go into
   `AcquiredCredential::metadata` or `OAuthCredential::provider_fields`, and
   the host writes them to the credential row; a secret a later refresh needs
   (a client secret the login registered for itself) goes into
   `OAuthCredential::provider_secrets`, which is sealed with the tokens and
   never published.

7. Put reusable wire mechanics in `src/channels/shared/` (cache magic
   strings, service caller helpers), gated on the features that use them.
   Policy stays in the channel; shared modules only execute what a channel
   asks for.

8. Test beside the code in `tests/<id>.rs` behind `#![cfg(feature = "...")]`.
   Build `ProviderView` and `CredentialView` by hand, call `prepare` with a
   fixture secret and assert the URL, injected auth and dropped headers;
   settle a shaped reply with `support::settled` to see the usage the host
   will read; parse recorded quota headers. A scripted `OutboundClient` (see `tests/capabilities.rs`)
   exercises multi-call overrides, and `tests/support/mod.rs` has a scripted
   `ServiceCaller`. Do not test core from here.

9. Register it in the host. Core never lists channels itself. A host that
   wants what it compiled in takes the whole list — this is what `gproxy-sdk`
   does, so enabling the feature there is the only step — and one that wants
   an exact set passes instances:

   ```rust
   let mut registry = ChannelRegistry::new();
   for channel in gproxy_channel::channels::compiled_in() {
       registry.register(channel)?;
   }
   let core = CoreBuilder::new(store).channels(registry).build().await?;

   // or, naming them:
   let core = CoreBuilder::new(store)
       .channel(Arc::new(gproxy_channel::channels::custom::Custom))?
       .channel(Arc::new(gproxy_channel::channels::acme::Acme))?
       .build()
       .await?;
   ```

   `ChannelRegistry` rejects a duplicate id, and a provider naming an
   unregistered channel is a configuration error rather than a fallback.

Finish with `cargo fmt --all`, `cargo clippy -p gproxy-channel --all-features
--all-targets -- -D warnings`, the same for `--target wasm32-unknown-unknown
--lib`, and `cargo test -p gproxy-channel --all-features`. A lint finding gets
a code change, not an `#[allow]`.
