---
title: "Providers & Credentials"
description: "Choose a channel, add providers and credentials, and configure login, allowance queries, and connections."
---

A **channel** is the compiled-in adapter for one upstream family. A
**provider** is one saved connection on a channel: a name, an optional base
URL, the channel's own `config` JSON, and a pool of credentials. Create as many
providers per channel as you need — `openai-main` and `openai-eu` can both be
on the `openai` channel.

Every write here is one transaction with the configuration revision bump, and
the instance reloads before it notifies its peers, so a change applies to the
next request without a restart.

## Supported channels

Only the channels compiled into your binary exist.
`GET /admin/api/channels` answers from the binary, never from the database, and
each entry carries the login modes, the capabilities and the `config` keys a
provider form should render.

| Channel | Upstream | Credential |
| --- | --- | --- |
| `aistudio` | Google AI Studio: the native Gemini methods and the `/v1beta/openai` compatibility layer off one origin | `{"api_key"}` |
| `antigravity` | A Google account through the Code Assist host the Antigravity editor talks to | OAuth |
| `aws_bedrock` | AWS Bedrock: the wire follows the model — `InvokeModel` for Anthropic models (event-stream translated to Claude SSE), the OpenAI-compatible Chat Completions for GPT, Grok, Qwen, DeepSeek and the rest | AWS key pair, or a Bedrock API key |
| `azure` | Azure OpenAI, and the Anthropic models Azure AI Foundry hosts | `{"api_key"}` |
| `claudeapi` | Anthropic's own API, plus its OpenAI compatibility layer and the cost report; `workspace_id` is required for a key not scoped to one workspace | `{"api_key", "quota_api_key"?, "workspace_id"?}` |
| `claudecode` | A Claude.ai subscription through the Claude Code CLI's requests | OAuth |
| `claudeweb` | A claude.ai browser session, rendered as Claude Messages SSE | session cookie + organization |
| `cline` | Cline's own account at `api.cline.bot` | `{"api_key"}` or OAuth |
| `cloudflare_ai_gateway` | Cloudflare AI Gateway over the REST API; account and gateway per credential, credit balance | `{"api_key", "account_id", "gateway_id"?}` |
| `codex` | A ChatGPT account through the Codex backend: Responses over SSE and WebSocket, `/wham/usage` | OAuth |
| `copilotcli` | GitHub Copilot through the `copilot` CLI | GitHub OAuth token |
| `custom` | Any API-key endpoint speaking OpenAI, Claude or Gemini natively | `{"api_key"}` |
| `dashscope` | Alibaba DashScope: the OpenAI and Anthropic modes, rerank, native image generation | `{"api_key"}` |
| `deepseek` | DeepSeek: Chat under `/v1`, Responses at the root, Claude Messages under `/anthropic` | `{"api_key"}` |
| `glm` | GLM pay-as-you-go API; live official ZCode model catalogue | `{"api_key"}` |
| `glmcode` | GLM Coding Plan; separate catalogue, Chat / Messages / Responses and subscription quota | `{"api_key"}` |
| `minimax` | MiniMax text API / Token Plan and H3 video creation, query, list, cancellation/deletion and download | `{"api_key"}` |
| `devin` | Devin (Windsurf) at `server.codeium.com`: Connect-RPC over protobuf | session token |
| `geminicli` | A Google account through the Code Assist endpoints the Gemini CLI talks to | OAuth |
| `grokbuild` | An xAI account through the Grok Build CLI | OAuth |
| `kimi` | Moonshot's platform with a key, or the Kimi Code subscription through a device login | `{"api_key"}` or OAuth |
| `kiro` | AWS CodeWhisperer through the Kiro desktop app | OAuth |
| `nvidia` | NVIDIA NIM: Chat Completions, models and embeddings | `{"api_key"}` |
| `openai` | OpenAI's own platform: the full surface, Responses and Realtime over a socket | `{"api_key", "quota_api_key"}` |
| `opencodego` | OpenCode Go: the subscription, open models, usage windows | `{"api_key"}` |
| `opencodezen` | OpenCode Zen: pay-as-you-go from the Console balance | `{"api_key"}` or OAuth |
| `openrouter` | OpenRouter: routing preferences in the body, the price the reply reports | `{"api_key"}` |
| `vercel` | Vercel AI Gateway: Chat, Responses, Claude Messages, embeddings and team credit balance | `{"api_key"}` |
| `vertex` | Google Vertex AI: the Google, Anthropic and OpenAI-compatible publishers | service-account key |
| `vertexexpress` | Vertex AI Express: the Gemini surface on one global origin | `{"api_key"}` |
| `workbuddy` | Tencent Copilot through its editor plugin | OAuth |
| `xai` | xAI (Grok): OpenAI Chat and Responses plus xAI's own TTS, STT and video | `{"api_key"}` |

Every channel builds for native targets **and** for
`wasm32-unknown-unknown`, so a Worker deployment is not a reduced channel set.

### Vendors that need no channel

A channel is code to maintain against somebody else's wire, and a vendor earns
one only when a provider row cannot state what it needs. A vendor whose whole
difference is an origin and a header is a `custom` provider:

```json
{ "name": "example-vendor", "channel": "custom",
  "baseUrl": "https://api.example-vendor.com",
  "config": { "dialects": ["openai_chat"] } }
```

NVIDIA NIM, Vercel AI Gateway and Cloudflare AI Gateway used to be served this
way; each has a channel now, because a row could not say everything they need.

## Provider configuration

| Field | Meaning |
| --- | --- |
| `name` | The operator's label, unique. It is also the **provider mount** and the left half of the `provider/model` form. |
| `channel` | One of the ids above. |
| `baseUrl` | The origin, when the channel takes one. |
| `config` | The channel's own JSON. Each channel declares which keys it decodes; `GET /admin/api/channels` is the list. |
| `connectionProfileId` | The outbound stack — proxy, TLS emulation — this provider's calls take. |
| `enabled` | A disabled provider leaves resolution. |

`config` for `custom` carries `dialects` (which wire shapes the endpoint
speaks), static `headers`, `allowed_headers` and the two magic-cache switches.
Channels that impersonate a vendor CLI declare their own identity headers and
keep them out of what a client can spoof.

### Claude model fallback

Set `fallback_mode` to `off` (default), `default`, or `models`, with
`fallback_models` holding an ordered list of upstream model IDs. The Console
renders these fields from the channel descriptor. Lists skip blanks, duplicates
and the primary model, and use at most three fallback models.

- **Claude API, Claude Code and Custom Claude endpoints:** send Anthropic's
  `fallbacks` and matching beta header. Default mode delegates to Anthropic.
- **OpenRouter:** send `fallbacks` for Claude Messages or `models` for Chat
  Completions. OpenRouter executes the model fallback. This does not change
  `provider.allow_fallbacks`, which controls supplier routing for the same model.
  Existing client `fallbacks` or `models` take precedence. Responses requests
  are not given an undocumented fallback parameter.
- **Vercel, Azure, Vertex and AWS Bedrock:** GProxy retries a Claude `refusal` on the
  next configured model through the same credential, with fresh channel
  preparation and signing. Default mode selects `claude-opus-4-8` in the
  primary model's namespace. For cloud-specific IDs, configure the full IDs
  accepted by that upstream.

Gateway fallback also applies after protocol conversion to Claude. It does not
retry arbitrary HTTP errors. Stream content is delivered immediately; after
output, continuation requires an upstream credit with a prefill claim. A server
tool is never replayed without a redeemable credit. Every physical attempt is
observed separately and priced by its model; a refusal reporting zero output
is unbillable. The final response retains the final attempt's top-level usage.

### Claude Code low-priority mode

`claudecode` with `low_priority: true` sends every Messages call the way the
CLI does after a user accepts its low-priority offer
(`anthropic-usage-limit: slow`), and a full five-hour window no longer takes
the credential out of rotation; the weekly windows still do. Anthropic decides
per account whether to serve the request at lower priority; a busy slot comes
back as a 429 and GProxy moves to the next credential.

### Per-operation URL overrides

`operation_endpoints` replaces the whole method URL for one
`(operation, dialect, transport)` on one provider. It is **not** a replacement
base URL with a default path appended; the channel's path parameters are
resolved by that method:

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/operation-endpoints \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","operation":"generate_content","dialect":"openai_chat",
       "url":"https://elsewhere.example/v1/chat/completions"}'
```

`{model}` in the URL is replaced with the upstream model, percent-encoded as
one path segment, for surfaces that put the model in the path:
`https://relay.example/v1beta/models/{model}:generateContent`.

`operation_rules` is the other half: a per-provider override of what the
channel does for an operation. Channel defaults stay in code and are not
copied into every new provider.
`POST /admin/api/providers/{id}/routing-defaults/reset` drops both in one
commit.

## A Credential Row

| Field | Meaning |
| --- | --- |
| `label` | Optional. Derived from the channel and the account when a login creates one. |
| `authKind` | `api_key`, `oauth` or `cookie` — how the secret was obtained. |
| `secret` | The channel's declared fields. Never returned; a listing reports only `hasSecret`. |
| `organizationId` / `teamId` / `userId` | The owner. Exactly one, or none for a shared credential. |
| `connectionProfileId` | Overrides the provider's outbound stack. |
| `expiresAtMs` | When the secret is due for refresh. |
| `status` | `active` or `dead`. A dead credential leaves the plan. |
| `version` | The compare-and-swap guard a refresh writes under. |

Secrets are sealed with AES-256-GCM under the master key, and the seal is
**bound to the row's own id**, so a blob copied onto another row does not open.
Reading one back is a separate, audited call:

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/credentials/{id}/reveal \
  -H "Authorization: Bearer $GPROXY_KEY"
```

With no master key, secrets are stored **unencrypted**. That is a supported
deployment, and the binary says so once at startup.

### Who can reach it

A credential is visible to a caller when it is **unowned**, or when its owner
matches the calling key's binding — the key's own user, team, or effective
organization. It is the *key's* binding rather than the holder's memberships
because a user can belong to two organizations while a key belongs to one. If
membership decided it, every key of a multi-org user would reach every
organization's subscriptions, and no key could ever be narrower than its
holder.

Visibility only narrows the live credential set; it never adds to it.

## Acquiring a Credential by Logging In

Channels that hold an account rather than a key offer one or more of three
flows. `GET /admin/api/channels` reports each channel's `loginModes`, and
asking for one a channel does not offer is refused.

| Flow | Steps | For |
| --- | --- | --- |
| Authorization code | start → complete | A browser redirect with PKCE |
| Device code | start → poll, repeatedly | A code typed on another device |
| Cookie exchange | exchange | A session cookie the person already has |

In the Console, open the provider's Credentials tab and choose **Add by signing in**.
The browser flow accepts the full callback URL; device-code polling is automatic.
The five POST operations live under `/admin/api/credential-login` and bind the
pending session to its initiating user and administrative scope. See
[Console administration](/guides/console/#credentials-and-upstream-sign-in).

GPROXY owns everything that is not the upstream's business. The **PKCE
verifier** is 32 random bytes minted locally, never sent; only its S256 digest
reaches the authorize URL, so an intercepted code is useless without it. The
**CSRF state** is minted and compared locally, and a state that does not match
is refused *and* destroys the session, so a replay cannot be retried into
success.

The pending session lives in the **shared cache**, not in this process. That is
what lets any instance finish a login another one started — behind a load
balancer it usually is another one — and what makes an abandoned login cost
nothing: the key expires, and an expired key is indistinguishable from an id
that was never issued.

**Polling is the caller's job.** A device poll performs one step and returns;
nothing sleeps or loops. The answer carries the interval to wait, and an
upstream `slow_down` raises it for every later poll.

A successful login is one ordinary credential insert through the same commit
primitive a management write uses. The secret is sealed **before** the
statement is built, so nothing between the channel and the database sees it in
the clear, and the answer is the credential's id and nothing else.

### Refresh

Channels that hold OAuth tokens declare when a secret is due, and the refresh
returns a **full replacement** — never a merge. The host persists it with a
compare-and-swap on `version` and publishes a credential-changed notice.

```sh
curl -s -X POST 'http://127.0.0.1:8787/admin/api/credentials/{id}/refresh?force=true' \
  -H "Authorization: Bearer $GPROXY_KEY"
```

An upstream that *definitively* refuses the credential (`invalid_grant`,
revoked) marks it dead. A transient transport or 5xx failure must not: it
surfaces as a transport error so the host retries later.

This path must never lose a write. Claude rotates the refresh token on every
refresh, which is why persistence is a version-guarded CAS rather than a
best-effort update.

## Health, Blocks and the Plan

Resolution drops disabled, retired and dead credentials outright. A
**blocked** credential — one an upstream rate-limited — is dropped while the
provider still has an unblocked one; a provider whose credentials are *all*
blocked keeps them and is ordered behind every healthy provider. A rate limit
is a last resort, not an outage.

```sh
# what the upstream says about this credential's windows
curl -s http://127.0.0.1:8787/admin/api/credentials/{id}/quota -H "Authorization: Bearer $GPROXY_KEY"
# ask the upstream now
curl -s -X POST http://127.0.0.1:8787/admin/api/credentials/{id}/quota-probe -H "Authorization: Bearer $GPROXY_KEY"
# redeem a reset credit, where the vendor sells them
curl -s -X POST http://127.0.0.1:8787/admin/api/credentials/{id}/quota-reset -H "Authorization: Bearer $GPROXY_KEY"
# forget the recorded health
curl -s -X POST http://127.0.0.1:8787/admin/api/credentials/{id}/health-reset -H "Authorization: Bearer $GPROXY_KEY"
# the operator limits covering it
curl -s http://127.0.0.1:8787/admin/api/credentials/{id}/limits -H "Authorization: Bearer $GPROXY_KEY"
```

`POST …/status` sets `active` or `dead` by hand, with a reason.

## Connection Profiles

A connection profile is the outbound stack a call takes: the proxy, and the
TLS and HTTP/2 identity it presents. It is selected credential-first, then
provider, then the channel's own default, then the instance default.

Six identities ship as presets, ready to store as a profile's `emulation`:

```sh
curl -s http://127.0.0.1:8787/admin/api/tls-presets -H "Authorization: Bearer $GPROXY_KEY"
```

```text
claude / Claude CLI    codex / Codex CLI       gemini / Gemini CLI
antigravity            kiro / Kiro CLI         copilot / GitHub Copilot CLI
```

Only the `wreq` transport presents one. A channel that impersonates a CLI
returns its own default; an explicit profile on the credential or the provider
still wins.

## The Two Probes

These are the only management calls that leave the process.

**Connectivity** asks Cloudflare's trace endpoint what this deployment looks
like from outside, through the client chain the scope names:

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/connectivity/test \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"scope":"global"}'
```

```json
{"ok":true,"latencyMs":803,"ip":"223.166.167.192","colo":"LAX","error":null}
```

The scope is `global`, `{"scope":"provider","provider_id":"…"}`,
`{"scope":"credential","credential_id":"…"}` — the very transport its calls
use — or `{"scope":"proxy","url":"…"}` for a proxy that is not configured
anywhere yet. **A network failure is `ok: false` with a reason, not an error**:
"the upstream is unreachable" is the answer that was asked for.

**Model test and discovery** go through the engine exactly as a caller's
request would:

```sh
curl -s -X POST http://127.0.0.1:8787/admin/api/models/test \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","model":"gpt-4o-mini"}'

curl -s -X POST http://127.0.0.1:8787/admin/api/models/discover \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…"}'
```

They **spend a real credential, consume real upstream quota, settle against any
budget that credential is subject to, and write a usage row**. There is no dry
run: a test that did not really call the upstream would not have tested
anything.

Discovery asks in the provider's own dialect, so the names are the upstream's.
Each comes back with whether this provider already has a row for it and whether
the bundled catalogue can price it;
`POST /admin/api/models/discover/apply` inserts the ones you pick, skipping
those already there.

## Moving a Configuration

```sh
gproxy export --out config.json --include-secrets
gproxy import --in  config.json --mode merge --source-master-key '…'
```

What travels is the configuration a deployment *is*: connection profiles,
providers, credentials, the model catalogue, routing, operation overrides,
rewrite rules, quotas, pricing and the settings row. **Identity does not
travel** — users, keys, organizations, teams, permissions and subscriptions
belong to the product layer — and neither do usage or captures, because copying
them would fabricate history the destination never had. See
[Configuration](/reference/configuration/#moving-a-configuration) for the
master-key rules.

## Request header allow-lists

Settings → Network provides a global request-header allow-list. Each provider
can add names with `config.allowed_headers`. The effective set is the **union**
of the global list, provider list and channel defaults, plus `content-type`.
Missing or empty lists add no entries; arbitrary client headers are dropped by
default. Source credentials and hop-by-hop headers remain excluded even when
listed. Channel-injected authentication and configured static `headers` are
independent of this client-header policy. Changes take effect on configuration
reload without rewriting the provider's saved list.

### GLM and MiniMax

`glm` uses the pay-as-you-go API; `glmcode` uses GLM Coding Plan. Both accept an `api_key` credential and default to `https://open.bigmodel.cn`. Global accounts use `baseUrl: "https://api.z.ai"`. `glmcode` supports the dedicated Chat, Claude Messages and Responses endpoints without falling back to paid API usage.

Both model lists come from ZCode's live official catalogue: request `/api/v1/client/configs`, download the returned `builtin_provider_config_json`, then select the regional platform API or Coding Plan template. There is no bundled model list or stale-list fallback. This is a product catalogue, not proof of a key's model entitlement.

`minimax` defaults to `https://api.minimax.io`; Chinese accounts can set their platform's API origin. OpenAI Chat and Claude Messages accept a Subscription Key (Coding Plan / Token Plan) or a pay-as-you-go API Key in `api_key`. Model discovery and detail use the official `/v1/models` and `/v1/models/{id}` endpoints. Separate providers can route subscription text and paid video independently.

`glmcode` queries `/api/monitor/usage/quota/limit` for reported percentages, independent windows and reset times. Platform `glm` does not yet implement account balance queries. MiniMax follows the official CLI's key-type selection of `/v1/token_plan/remains` or `/account/query_balance`. These readings are currently for display; upstream responses determine exhaustion.

MiniMax video covers **H3 V2** (`MiniMax-H3`, `MiniMax-H3-Max`) and requires a pay-as-you-go API Key. Submit the GPROXY OpenAI video extension JSON to `/v1/videos`:

```json
{
  "model": "MiniMax-H3",
  "prompt": "A cat running through a garden",
  "duration": 5,
  "resolution": "2K",
  "aspect_ratio": "16:9"
}
```

Query `/v1/videos/{id}` using the returned ID and download completed output from `/v1/videos/{id}/content`. `frame_images` supports first/last frames; `input_references` supports image, video and audio references. Native MiniMax `content` is also accepted. Sora multipart, `seconds`, `size`, and legacy Hailuo 2.x V1 endpoints are not supported.

Lists use `page_num` / `page_size` (`limit` maps to `page_size`); `after` / `order` cursors are unsupported. Deletion preserves the upstream `action`: `cancelled` cancels a queued task, while `deleted` removes a task record. The channel does not poll automatically or store video files.

Sources: [ZCode catalogue implementation](https://github.com/zai-org/ZCode/blob/29628c9acdb81b703bbd4080c207a0e7ce5e276e/packages/provider-node/src/zcode-builtin-download.ts), [MiniMax official CLI](https://github.com/MiniMax-AI/cli), [GLM Coding Plan](https://docs.z.ai/devpack/tool/others), [MiniMax text](https://platform.minimax.io/docs/api-reference/text-anthropic-api), [MiniMax H3 V2](https://platform.minimax.io/docs/api-reference/video-generation-v2-create).
