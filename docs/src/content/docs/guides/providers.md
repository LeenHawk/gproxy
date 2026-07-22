---
title: Providers and Channels
description: Configure upstream providers, credentials, routing capabilities, proxies, TLS profiles, and scoped provider access in GPROXY v2.
---

A **provider** is a saved upstream connection. It has a name, a channel type,
one or more credentials, a model catalogue, and optional routing and request
rules. For example, you might create `openai-primary`, `openrouter-fallback`, and
`claude-team` as separate providers even when more than one uses the same
channel type.

Changes made in the console take effect for new requests without restarting
GPROXY.

## Built-in Channels

Native builds include every channel below. Edge builds exclude the consumer-web
channels that require multi-step WebSocket sessions. Current built-in channel ids are:

| Channel id | Typical use |
| --- | --- |
| `openai`, `custom` | OpenAI API or OpenAI-compatible gateways. |
| `azure` | Microsoft Foundry / Azure OpenAI, including OpenAI v1, Claude, embeddings, compact, and deployment-bound image APIs. |
| `bedrock-mantle`, `bedrock-runtime` | Amazon Bedrock API-key channels for the regional Mantle and Runtime inference surfaces. |
| `openrouter`, `deepseek`, `groq`, `nvidia`, `vercel` | API-key providers with OpenAI-like surfaces. |
| `claudeapi` | Anthropic Claude Messages API. |
| `aistudio`, `vertex`, `vertexexpress` | Gemini / Vertex upstreams; `vertex` also supports native Claude partner models. |
| `codex`, `claudecode`, `geminicli`, `antigravity`, `grokbuild`, `kiro`, `copilotcli` | OAuth, device-code, cookie, or envelope-style agent channels. |
| `chatgpt` | ChatGPT consumer web backend via a chatgpt.com session cookie. |
| `claudeweb` | Claude consumer web backend via a claude.ai session cookie (native only). |

Every channel declares a routing surface as `(Operation, OperationKind) ->
RoutingDecision`. That is the source for the provider's default
`routing_rules` rows. Request behavior is therefore described by operation
capability, not by provider-family buckets.

### Azure channel

The `azure` channel uses API-key credentials. Set `settings_json.base_url` to
the Azure resource root, such as `https://<resource>.openai.azure.com`, or to
the Foundry endpoint shown in the portal. OpenAI-family requests are mapped to
`/openai/v1/*` with the `api-key` header. Claude Messages and Count Tokens are
mapped to `/anthropic/v1/*` with `x-api-key`. The routed upstream model ID must
be the Azure deployment name.

Image generation and editing use Azure's current deployment-bound endpoints:
`/openai/deployments/{deployment}/images/generations` and
`/openai/deployments/{deployment}/images/edits`. The default `api-version` is
`2025-04-01-preview`; override it with `settings_json.api_version` or place it
directly in an exact endpoint URL. Azure's Responses schema includes the
compaction type for `/openai/v1/responses/compact`; if a resource version has
not enabled that operation, set `endpoints.openai_compact` to the complete URL
supported by that resource.

If OpenAI and Claude deployments use different resource hosts, configure their
complete URLs separately in `settings_json.endpoints`. Exact endpoints take
precedence over `base_url` and support a `{model}` deployment placeholder.

Azure also supports all three prompt-management features. Provider
`cache_breakpoint` rules insert native `prompt_cache_breakpoint` markers into
OpenAI Chat/Responses or `cache_control` into Claude Messages. Enabling
`enable_magic_cache` recognizes the shared GPROXY trigger strings and inserts
the matching native marker. Enabling `enable_claude_fable_fallback` adds a
server-side fallback from the `claude-fable-5` deployment to
`claude-opus-4-8`, including the required Anthropic beta header. Custom Azure
deployment names must use an explicit `fallbacks` chain plus the
`server-side-fallback-2026-06-01` beta header because the automatic mapping only
recognizes those standard deployment names.

### Amazon Bedrock channels

The `bedrock-mantle` and `bedrock-runtime` channels use Amazon Bedrock API keys,
stored as `{"api_key":"..."}`. Supply the Bedrock bearer token commonly exposed
as `AWS_BEARER_TOKEN_BEDROCK`, not an IAM access-key ID. SigV4 credentials and
the separate Claude Platform on AWS integration are not used by these channels.

Set `settings_json.region` to the AWS region containing the models. It defaults
to `us-east-1`. GPROXY derives
`https://bedrock-mantle.<region>.api.aws` or
`https://bedrock-runtime.<region>.amazonaws.com`; `base_url` and exact
`endpoints` can still override those defaults.

`bedrock-mantle` uses the AWS OpenAI-compatible Models, Chat Completions, and
Responses APIs. Native Claude requests use Mantle's `/anthropic/v1/messages`
surface. OpenAI Compact requests fall back to a regular Mantle Responses call
and are converted back to the Compact response shape.

`bedrock-runtime` uses AWS's OpenAI-compatible Models and Chat Completions APIs.
Native Claude requests use `InvokeModel`; GPROXY places the routed Bedrock model
or inference-profile ID in `/model/{modelId}/invoke`, adds
`anthropic_version: bedrock-2023-05-31`, and uses
`/model/{modelId}/invoke-with-response-stream` for true Claude streaming. The
channel incrementally decodes AWS EventStream frames into Claude SSE while
leaving Runtime's OpenAI Chat Completions SSE byte-for-byte intact. Claude Count
Tokens uses Runtime's `/model/{modelId}/count-tokens` operation. Compact requests
use Bedrock's Anthropic compaction beta through the non-streaming InvokeModel
path.

Both channels support provider `cache_breakpoint` rules, magic-string cache
triggers, and the opt-in Fable 5 to Opus 4.8 fallback. Bedrock-style model IDs
such as `anthropic.claude-fable-5` retain their dot namespace when GPROXY builds
the fallback. Model and API availability remains region-dependent.

### Vertex Claude partner models

The `vertex` channel accepts Claude's native `/v1/messages` and
`/v1/messages/count_tokens` interfaces in addition to Gemini. Configure the
service-account credential as usual, set `location` to a region where the
selected Claude model is available, and use the Vertex model id (for example,
an id ending in `@YYYYMMDD`) as the route member's upstream model. GPROXY keeps
the Anthropic request and SSE response formats native while mapping the call to
Vertex's `publishers/anthropic` raw-prediction endpoint. See Google's
[partner-model overview](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/partner-models/use-partner-models)
and [Claude model documentation](https://docs.cloud.google.com/gemini-enterprise-agent-platform/models/partner-models/claude)
for enablement, model ids, and regional availability.

Providers created before this capability was added keep their stored routing
rules. Reset that provider's routing defaults, or change the Claude Messages and
Claude count-tokens rules to `passthrough`, to opt in to the native endpoints.

### ChatGPT channel (cookie session)

The `chatgpt` channel proxies the **chatgpt.com consumer web backend** using a
browser **session cookie** — no API key or OAuth. It supports normal chat,
thinking / pro / deep-research (streamed chain-of-thought + report), web search,
and image generation/edit.

**Getting the credential.** Sign in to <https://chatgpt.com> in a browser, open
DevTools → Network, click any `chatgpt.com` request, and copy its full `Cookie`
request header. In the console, add a `chatgpt` provider with **Cookie login** and
paste that cookie string. gproxy exchanges it at `/api/auth/session` to mint the
access token and warms the Cloudflare / sentinel anti-bot state into the sealed
secret. gproxy then auto-refreshes the access token from the stored cookie as it
nears expiry (the JWT lasts ~10 days; the session cookie far longer), so the
credential lives as long as the browser session — re-paste only when the session
cookie itself lapses.

**Session mode.** A per-provider setting (`provider_settings.mode`), surfaced in
the provider form as a three-way selector, controls where conversations land:

| Mode | Behavior |
| --- | --- |
| Normal | Persistent conversations in your normal chat history. |
| Temporary (default) | Temporary chat — excluded from history and model training. |
| Project | Conversations open inside a ChatGPT **project**, auto-created/found by name (default `gproxy`), so they stay grouped for easy review. Set the project name in the form. |

Project and Temporary are mutually exclusive (a project conversation is always
persistent). The legacy `temporary_chat: true\|false` boolean is still honored
when `mode` is absent.

## Provider Fields

The provider record carries:

| Field | Meaning |
| --- | --- |
| `name` | Unique provider name. Scoped routes use this in the URL. |
| `channel` | Channel registry id, such as `openai` or `claudeapi`. |
| `settings_json` | Free-form channel settings. Common keys include `base_url`, `endpoints`, and channel toggles. |
| `credential_strategy` | Credential-pool strategy, currently `round_robin` or `sticky`. |
| `proxy_url` | Native outbound proxy fallback for the provider. Edge ignores native proxy settings. |
| `tls_fingerprint` | Optional provider-level TLS/HTTP2 emulation profile. Credential settings can override it. |
| `enabled` | Disabled providers disappear from routing. |

Common `settings_json` values are available as fields in the console:

| Setting | Use |
| --- | --- |
| `base_url` | Channel-wide fallback prefix. The standard operation path is appended when no exact endpoint override exists. |
| `endpoints` | Optional exact URL overrides, for example `{"openai_chat_completions":"https://api.openai.com/v1/chat/completions"}`. Overrides take precedence over `base_url`, and no path is appended. Dynamic model paths may use `{model}`. |
| `api_version` | API version for `azure` image generation and editing; defaults to `2025-04-01-preview`. |
| `region` | AWS region for `bedrock-mantle` and `bedrock-runtime`; defaults to `us-east-1`. |
| `enable_magic_cache` | Recognize GPROXY cache trigger strings and write native Claude or OpenAI cache breakpoints. Available for OpenAI, Azure, Amazon Bedrock, Codex, Claude API, Claude Code, OpenRouter, and Vercel. |
| `enable_claude_fable_fallback` | Add the supported Claude Fable-to-Opus fallback behavior on Claude-capable channels, including Azure and Amazon Bedrock. |

See [Prompt Caching](/guides/claude-caching/) before enabling magic-string
caching, especially for OpenAI's model and TTL requirements.

Credential rows belong to a provider. They carry `kind`, sealed `secret_json`,
`weight`, optional `rpm_limit` / `tpm_limit`, optional proxy and TLS overrides,
and an `enabled` flag. Secrets are redacted in debug output and sealed when a
master key is configured.

## Aggregated and Scoped Access

GPROXY v2 supports two ways to reach an upstream:

| Mode | URL shape | Resolution |
| --- | --- | --- |
| Aggregated | `/v1/*`, `/v1beta/*` | The request `model` resolves through alias / route tables, then to a route member and credential. |
| Scoped | `/{provider}/v1/*`, `/{provider}/v1beta/*` | The provider name comes from the path; the model goes directly to that provider. |

Both modes use the same classifier, auth, transform, process, channel, and
settle layers after resolution. Aggregated mode is the normal multi-provider
gateway path. Scoped mode is useful for debugging or exposing one provider
without creating a route.

## Routing Rules

Routing rules are provider-local. Each row names:

- `operation`: for example `generate_content`, `stream_generate_content`,
  `count_tokens`, or `create_embedding`.
- `kind`: one of the content-generation wire kinds
  `open_ai_responses`, `open_ai_chat_completions`, `claude_messages`,
  `gemini_generate_content`, or provider kinds `open_ai`, `claude`, `gemini`.
- `implementation`: `passthrough`, `transform_to`, `local`, or `unsupported`.
- optional `dest_operation` and `dest_kind` for `transform_to`.

No matching routing rule means `unsupported`. Defaults are materialized into
stored rows when a provider is created, and the console can reset them from the
channel defaults.

## Provider Rule Sets

Attach reusable rule sets to a provider to add system text, cache breakpoints,
field rewrites, text transforms, or headers. Rules run after protocol conversion
and before the request is sent upstream. Invalid or non-applicable rules are
logged and skipped instead of failing the request.
