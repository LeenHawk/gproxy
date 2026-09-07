---
title: "Providers & Credentials"
description: "Channels, providers, credential pools, the login wizard, token refresh, health tracking, and provider tooling in the console"
---

A **channel** is a compiled-in adapter for one upstream API family. A
**provider** is a saved connection that uses a channel: a name, settings, and a
pool of credentials. Create as many providers per channel as you need, for
example `openai-main` and `openai-eu` both on the `openai` channel.

Changes saved in the console apply to new requests without a restart.

## Channels

The console reads the channel list from the running binary. Each channel
declares its display name, the routes it can serve, the provider settings it
accepts, the credential fields it needs, and whether it offers a login wizard.
The 28 channel ids, grouped by credential shape:

| Credential shape | Channel ids |
| --- | --- |
| API key (`api_key`) | `aistudio`, `azure`, `claudeapi`, `cloudflare-ai-gateway`, `custom`, `dashscope`, `deepseek`, `groq`, `nvidia`, `openai`, `openrouter`, `vercel`, `vertexexpress`, `xai` |
| API key or OAuth token | `cline`, `kimi`, `opencode` |
| OAuth token (`access_token`, `refresh_token`) | `claudecode`, `codex`, `grokbuild`, `kiro`, `workbuddy` |
| Google OAuth (`access_token`, `refresh_token`, `project_id`) | `antigravity`, `geminicli` |
| Google service account (`client_email`, `private_key`, `project_id`) | `vertex` |
| AWS (`api_key`, or `access_key_id` + `secret_access_key` + optional `session_token`) | `aws-bedrock` |
| GitHub token (`github_token`) | `copilotcli` |
| Browser cookie (`cookie`, `account_uuid`) | `claudeweb` (native builds only) |

Two v2 id pairs are canonicalized on import: `kimiapi` and `kimicode` become
`kimi`; `opencodezen` and `opencodego` become `opencode` with `tier` set to
`zen` or `go`. `claudeweb` is not compiled into edge builds.

## Provider Fields

| Field | Meaning |
| --- | --- |
| Route name (`name`) | Unique identifier. It is also the named prefix in URLs, for example `/openai-main/v1/chat/completions`. |
| Display name (`label`) | Optional text shown in the console. |
| Channel | One of the ids above. Fixed after creation. |
| Credential strategy | `round_robin` (default) or `sticky`. See below. |
| Provider proxy URL | Overrides the instance proxy for this provider. Credentials can override it again. |
| Client fingerprint | Optional TLS/HTTP profile: a preset or custom JSON. Credentials can override it. |
| Forwarded metadata | Which caller headers and query parameters pass upstream, and which response headers return. Defaults come from the channel. |
| Enabled | Disabled providers leave routing. |

### Channel Settings

Settings are stored as one JSON object. The channel declares typed fields for
the common keys; everything else is reachable through **Edit settings JSON**.

| Key | Channels | Meaning |
| --- | --- | --- |
| `base_url` | all | Upstream origin used when no exact endpoint override exists. |
| `auto_refresh_models` | all | Ask this provider for its live model list when a client lists models. Default `true`. |
| `endpoints` | all | Exact URL per operation kind. See below. |
| `enable_openai_magic_cache` | `openai`, `codex`, `azure`, `custom`, `openrouter`, `vercel`, `opencode` | Recognize the cache trigger strings on OpenAI targets. |
| `enable_claude_magic_cache` | `claudeapi`, `claudecode`, `azure`, `custom`, `openrouter`, `vercel`, `aws-bedrock`, `opencode` | Recognize the cache trigger strings on Claude targets. |
| `claude_fallback_mode`, `claude_fallback_models` | `claudeapi`, `claudecode`, `custom`, `openrouter`, `vercel` | `off`, `default`, or `models` with an ordered list. |
| `region`, `video_output_s3_uri` | `aws-bedrock` | AWS region; S3 destination for generated video. |
| `region`, `profile_arn`, `auth_base_url` | `kiro` | AWS region, profile ARN, authentication origin. |
| `location`, `oauth_client_id`, `oauth_client_secret`, `oauth_token_url` | `vertex`, `antigravity`, `geminicli` | Google Cloud region and OAuth client overrides. |
| `tier`, `console_base_url` | `opencode` | `zen` or `go`; console origin for device login. |

Exact endpoint overrides live under `endpoints`, keyed by operation kind, and
take precedence over `base_url`. No path is appended; `{model}` is replaced
with the upstream model id:

```json
{
  "endpoints": {
    "openai_chat_completions": "https://api.example/v1/chat/completions",
    "gemini_generate_content": "https://api.example/v1beta/models/{model}:generateContent"
  }
}
```

The console offers only the kinds the channel can serve. Common keys are
`openai_chat_completions`, `openai_responses`, `claude_messages`,
`gemini_generate_content`, `gemini_stream_generate_content`,
`openai_list_models`, `openai_embeddings`, `image_generations`.

## Credentials

A credential belongs to one provider and carries:

| Field | Meaning |
| --- | --- |
| Label | Optional; use it to identify the upstream account. |
| Kind | `api_key`, `oauth`, or `cookie`. Records how the secret was obtained. |
| Secret | A single key pasted as-is, or a JSON object with the channel's declared fields. Editing prefills the stored secret; leaving it blank keeps the stored value. |
| Traffic weight | Default 100. Share of traffic inside the pool. |
| Requests per minute, Tokens per minute | Optional per-credential ceilings. |
| Proxy override URL | Replaces the provider and instance proxy for this credential. |
| Client fingerprint | Replaces the provider fingerprint. |
| Enabled | Disabled credentials are skipped. |

Secrets are sealed at rest when a master key is configured (see
[Configuration](/reference/configuration/)). Reading a stored secret back in
the console is a separate, audited action.

### Pool Strategy

Selection is a deterministic counter rotation, never random.

| Strategy | Behaviour |
| --- | --- |
| `round_robin` | Each request advances a counter per route member; the counter picks a credential in proportion to weight. |
| `sticky` | The slot is derived from the caller's API key (or its session id when the client sends one), so one key stays on one credential until the pool changes. |

### Per-Credential Limits

RPM and TPM are enforced in fixed 60-second windows through the cache backend.
TPM counts the request's input tokens with the tokenizer ladder before the
upstream call. A request over either limit fails with `429` and does not
consume the window. These limits protect an upstream account; caller limits
live in [Permissions, Rate Limits & Quotas](/guides/permissions/).

### Health

Health is recorded per (credential, upstream model) from the upstream
response: `2xx` marks healthy, `429` and `5xx` mark degraded, `401`-`403` mark
dead. A failed token refresh marks the whole credential degraded; a malformed
secret marks it dead. Degraded credentials sort after healthy ones in the same
tier; dead ones are removed from the plan. A newer observation replaces the
older one, and observations recorded against a previous credential version are
ignored, so re-saving the secret starts clean. The credential card shows the
worst current state, the abnormal models, the last status and detail, and a
**Clear health state** action.

### Token Refresh

Channels that hold OAuth tokens declare when a secret is due. Refresh runs
under an exclusive 60-second lease in the cache, so concurrent requests refresh
once; the other requests poll every second and pick up the new version. The
rotated secret is persisted with a version guard. Claude rotates the refresh
token on every refresh, so this path must never lose a write.

## Login Wizard

Two channels acquire their first credential through the console:

| Channel | Modes |
| --- | --- |
| `codex` | Browser sign-in (authorization code + PKCE), Device code |
| `claudecode` | Browser sign-in (authorization code + PKCE), Browser cookie (native builds only) |

- **Browser sign-in**: GPROXY builds the authorization URL with an S256 PKCE
  challenge and state. Approve in the browser, then paste the full callback
  URL back into the wizard. The code exchange stores access and refresh tokens.
- **Device code**: the wizard shows a user code and the vendor verification
  page, then polls until the vendor reports approval or denial.
- **Browser cookie**: paste the full `Cookie` header or the `sessionKey`
  value. GPROXY discovers the organization, runs the OAuth exchange with the
  cookie, and keeps the cookie sealed for re-login when the tokens expire.

Every other channel takes a pasted key or token.

## Credential spending limits

Open **Providers → Credentials → Spending limits** to set independent total,
monthly, weekly, and daily USD caps. Leave a field blank for unlimited spend;
zero immediately blocks paid requests. Reaching any cap excludes that credential
from new paid attempts. Routing may select another credential; if every candidate
is exhausted, the request returns HTTP 402. Free operations remain available.

Limits use gateway model pricing and begin counting when configured. Missing
model pricing blocks paid requests on a limited credential. Counters persist
independently of usage logging; disabling enforcement keeps counting, and editing
limits preserves existing spend. Total spend never resets automatically. Daily,
weekly, and monthly caps reset at 00:00 UTC each day, Monday, and the first of the
month respectively. The Console displays reset times in your local timezone.

Before a paid request is sent, its estimated cost (input tokens at the model's
price) is reserved against the credential's windows, so concurrent requests
see each other and cannot together overrun a cap by more than the difference
between estimates and settled cost; the reservation is released when the
request settles or is abandoned. Output tokens are only known at settlement,
so a long response can still finish past the cap. These counters do not
include requests made outside this gateway or reconcile provider invoices.

## Tools

- **Connectivity test**: probes `https://1.1.1.1/cdn-cgi/trace` (IPv4 and
  IPv6) through the proxy that would apply at the chosen scope, and reports
  the egress IP, location, latency, and which proxy source was used.
- **Upstream quota**: for channels that expose it (`codex`, `claudecode`,
  `geminicli`), the credential card shows observed quota windows with used
  percent and period end. **Refresh** probes the upstream account live;
  Codex reset credits can be consumed from the card. A credential with a live
  window at 90% or more sorts behind its peers in the same failover tier, and
  at 100% it sorts last.
- **Batch actions**: enable, disable, and delete apply to selected providers
  and credentials in one call, with a per-item outcome.
- **Export / import**: a configuration export covers identity, providers,
  credentials, keys, quotas, pricing, routes, aliases, and rules. Secrets are
  omitted unless requested; a secret-bearing export records whether it was
  plaintext or sealed and under which key fingerprint. Import re-seals under
  the local key and reports skipped credentials and keys.

Models, pull-from-upstream, and the model test are covered in
[Models, Routes & Aliases](/guides/models/).
