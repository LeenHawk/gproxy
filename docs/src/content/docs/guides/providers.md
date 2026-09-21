---
title: "Providers & Credentials"
description: "The 25 channels, what a provider row holds, credential pools and their lifecycle, the three login flows, connection profiles, and the two probes."
---

A **channel** is the compiled-in adapter for one upstream family. A
**provider** is one saved connection on a channel: a name, an optional base
URL, the channel's own `config` JSON, and a pool of credentials. Create as many
providers per channel as you need — `openai-main` and `openai-eu` can both be
on the `openai` channel.

Every write here is one transaction with the configuration revision bump, and
the instance reloads before it notifies its peers, so a change applies to the
next request without a restart.

## The 25 Channels

Only the channels compiled into your binary exist.
`GET /admin/api/channels` answers from the binary, never from the database, and
each entry carries the login modes, the capabilities and the `config` keys a
provider form should render.

| Channel | Upstream | Credential |
| --- | --- | --- |
| `aistudio` | Google AI Studio: the native Gemini methods and the `/v1beta/openai` compatibility layer off one origin | `{"api_key"}` |
| `antigravity` | A Google account through the Code Assist host the Antigravity editor talks to | OAuth |
| `aws_bedrock` | AWS Bedrock: SigV4-signed `InvokeModel` for the Anthropic families, event-stream replies translated to Claude SSE | AWS key pair, or a Bedrock API key |
| `azure` | Azure OpenAI, and the Anthropic models Azure AI Foundry hosts | `{"api_key"}` |
| `claudeapi` | Anthropic's own API, plus its OpenAI compatibility layer and the cost report | `{"api_key", "quota_api_key"}` |
| `claudecode` | A Claude.ai subscription through the Claude Code CLI's requests | OAuth |
| `claudeweb` | A claude.ai browser session, rendered as Claude Messages SSE | session cookie + organization |
| `cline` | Cline's own account at `api.cline.bot` | `{"api_key"}` or OAuth |
| `codex` | A ChatGPT account through the Codex backend: Responses over SSE and WebSocket, `/wham/usage` | OAuth |
| `copilotcli` | GitHub Copilot through the `copilot` CLI | GitHub OAuth token |
| `custom` | Any API-key endpoint speaking OpenAI, Claude or Gemini natively | `{"api_key"}` |
| `dashscope` | Alibaba DashScope: the OpenAI and Anthropic modes, rerank, native image generation | `{"api_key"}` |
| `deepseek` | DeepSeek: Chat under `/v1`, Responses at the root, Claude Messages under `/anthropic` | `{"api_key"}` |
| `devin` | Devin (Windsurf) at `server.codeium.com`: Connect-RPC over protobuf | session token |
| `geminicli` | A Google account through the Code Assist endpoints the Gemini CLI talks to | OAuth |
| `grokbuild` | An xAI account through the Grok Build CLI | OAuth |
| `kimi` | Moonshot's platform with a key, or the Kimi Code subscription through a device login | `{"api_key"}` or OAuth |
| `kiro` | AWS CodeWhisperer through the Kiro desktop app | OAuth |
| `openai` | OpenAI's own platform: the full surface, Responses and Realtime over a socket | `{"api_key", "quota_api_key"}` |
| `opencode` | OpenCode Zen and Go | `{"api_key"}` or OAuth |
| `openrouter` | OpenRouter: routing preferences in the body, the price the reply reports | `{"api_key"}` |
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
{ "name": "nvidia-nim", "channel": "custom",
  "baseUrl": "https://integrate.api.nvidia.com",
  "config": { "dialects": ["openai_chat"] } }
```

```json
{ "name": "vercel-gateway", "channel": "custom",
  "baseUrl": "https://ai-gateway.vercel.sh/v1",
  "config": { "dialects": ["openai_chat", "openai", "claude"] } }
```

```json
{ "name": "cf-gateway", "channel": "custom",
  "baseUrl": "https://api.cloudflare.com/client/v4/accounts/{account_id}/ai",
  "config": { "dialects": ["openai_chat", "openai", "claude"],
              "headers": { "cf-aig-gateway-id": "default" } } }
```

## A Provider Row

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

### Per-operation URL overrides

`operation_endpoints` replaces the whole method URL for one
`(operation, dialect, transport)` on one provider. It is **not** a replacement
base URL with a default path appended; the channel's path parameters are
resolved by that method:

```sh
curl -s -X POST http://127.0.0.1:7070/admin/api/operation-endpoints \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","operation":"generate_content","dialect":"openai_chat",
       "url":"https://elsewhere.example/v1/chat/completions"}'
```

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
curl -s -X POST http://127.0.0.1:7070/admin/api/credentials/{id}/reveal \
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
curl -s -X POST 'http://127.0.0.1:7070/admin/api/credentials/{id}/refresh?force=true' \
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
curl -s http://127.0.0.1:7070/admin/api/credentials/{id}/quota -H "Authorization: Bearer $GPROXY_KEY"
# ask the upstream now
curl -s -X POST http://127.0.0.1:7070/admin/api/credentials/{id}/quota-probe -H "Authorization: Bearer $GPROXY_KEY"
# redeem a reset credit, where the vendor sells them
curl -s -X POST http://127.0.0.1:7070/admin/api/credentials/{id}/quota-reset -H "Authorization: Bearer $GPROXY_KEY"
# forget the recorded health
curl -s -X POST http://127.0.0.1:7070/admin/api/credentials/{id}/health-reset -H "Authorization: Bearer $GPROXY_KEY"
# the operator limits covering it
curl -s http://127.0.0.1:7070/admin/api/credentials/{id}/limits -H "Authorization: Bearer $GPROXY_KEY"
```

`POST …/status` sets `active` or `dead` by hand, with a reason.

## Connection Profiles

A connection profile is the outbound stack a call takes: the proxy, and the
TLS and HTTP/2 identity it presents. It is selected credential-first, then
provider, then the channel's own default, then the instance default.

Six identities ship as presets, ready to store as a profile's `emulation`:

```sh
curl -s http://127.0.0.1:7070/admin/api/tls-presets -H "Authorization: Bearer $GPROXY_KEY"
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
curl -s -X POST http://127.0.0.1:7070/admin/api/connectivity/test \
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
curl -s -X POST http://127.0.0.1:7070/admin/api/models/test \
  -H "Authorization: Bearer $GPROXY_KEY" -H 'content-type: application/json' \
  -d '{"providerId":"…","model":"gpt-4o-mini"}'

curl -s -X POST http://127.0.0.1:7070/admin/api/models/discover \
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
