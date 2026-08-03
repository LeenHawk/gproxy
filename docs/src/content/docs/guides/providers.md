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
channels that require multi-step WebSocket sessions. Cookie login for Claude is
native-only; an edge runtime does not advertise that login mode. Current built-in
channel ids are:

| Channel id | Typical use |
| --- | --- |
| `openai`, `custom` | OpenAI API or OpenAI-compatible gateways. |
| `azure` | Microsoft Foundry / Azure OpenAI, including OpenAI v1, Claude, embeddings, compact, and deployment-bound image APIs. |
| `aws-bedrock` | Amazon Bedrock API-key channel using native Bedrock control-plane and Runtime APIs. |
| `openrouter`, `deepseek`, `groq`, `nvidia`, `vercel` | API-key providers with OpenAI-like surfaces. |
| `claudeapi` | Anthropic Claude Messages API. |
| `aistudio`, `vertex`, `vertexexpress` | Gemini / Vertex upstreams; `vertex` also supports native Claude partner models. |
| `codex`, `claudecode`, `geminicli`, `antigravity`, `grokbuild`, `kiro`, `copilotcli` | OAuth, device-code, cookie, or envelope-style agent channels. |
| `claudeweb` | Claude consumer web backend via a claude.ai session cookie (native only). |

This table describes the official builds. A custom native binary can also link
external channel crates. The Console reads the authenticated runtime catalog,
so an external entry appears in the provider selector from its
`Channel::metadata()` result without editing the static Console list. See
[Adding a Channel](/guides/adding-a-channel/) for the compile-time integration
contract.

The successful runtime catalog is authoritative for the exact executable,
including credential family, login modes, endpoint kinds, secret template, and
usage support. The Console adds only UI hints for known built-ins. If a confirmed
older backend returns 404 or 405 for the catalog endpoint, the Console uses its
static list only to describe existing built-in providers. This fallback is
display-only: provider creation and saving, manual credential creation and
editing, OAuth, and bulk credential import stay disabled. Usage viewing and
metadata-independent delete or batch actions remain available. Loading applies
only before the first successful catalog response; cached authoritative metadata
remains usable during background refetches. Other catalog failures are shown
separately and can be retried. The Admin API also rejects provider create or
update requests whose channel is not registered in the running executable.

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

Azure also supports both prompt-management features. Provider
`cache_breakpoint` rules insert native `prompt_cache_breakpoint` markers into
OpenAI Chat/Responses or `cache_control` into Claude Messages. Enable
`enable_openai_magic_cache` and `enable_claude_magic_cache` independently to
recognize the shared GPROXY trigger strings on each target protocol and insert
the matching native marker. Anthropic server-side fallback is not available on
Microsoft Foundry, so the Azure channel does not inject `fallbacks`.

### DeepSeek channel

The `deepseek` channel supports DeepSeek's native OpenAI-compatible Responses
API in both non-streaming and HTTP/SSE streaming modes. Clients call GPROXY's
normal `POST /v1/responses` endpoint; the channel maps it to DeepSeek's
documented `POST https://api.deepseek.com/responses` endpoint and authenticates
with `Authorization: Bearer`. Responses WebSocket clients are bridged to that
HTTP/SSE endpoint because DeepSeek documents HTTP streaming, not a WebSocket
Responses transport. `settings_json.endpoints.openai_responses` remains
available as an exact URL override.

DeepSeek's current official documentation says Responses supports only
`deepseek-v4-flash`; `deepseek-v4-pro` support is planned for early August 2026.
The API is stateless: `previous_response_id`, `conversation`, storage, and
background execution are unsupported. It supports
text/reasoning, function calls, server-side web search, and the `apply_patch`
custom tool used by Codex, but not image or file input. Other unsupported
Responses parameters and item types are generally ignored. See DeepSeek's
[Responses guide](https://api-docs.deepseek.com/guides/responses_api) for the
current compatibility table.

Providers created before this capability was added retain their stored routing
rules. Reset the provider's routing defaults to opt in to all native Responses
and WebSocket-bridge routes.

### Amazon Bedrock channel

The `aws-bedrock` channel uses an Amazon Bedrock API key stored as
`{"api_key":"..."}`. Supply the Bedrock bearer token commonly exposed as
`AWS_BEARER_TOKEN_BEDROCK`, not an IAM access-key ID. Every request uses
`Authorization: Bearer`; the channel does not use SigV4 credentials, Mantle, or
the separate Claude Platform on AWS integration.

Set `settings_json.region` to the AWS region containing the models. It defaults
to `us-east-1`. GPROXY derives the control-plane endpoint
`https://bedrock.<region>.amazonaws.com` and Runtime endpoint
`https://bedrock-runtime.<region>.amazonaws.com`. `control_base_url` and
`base_url` override those roots respectively; exact `endpoints` take precedence.

Model listing and lookup use `ListFoundationModels` and `GetFoundationModel`.
OpenAI Chat Completions, OpenAI Responses, Claude Messages, and Gemini content
requests converge on Runtime Converse. Streaming uses ConverseStream and GPROXY
incrementally decodes AWS EventStream into the requested downstream SSE format.
Text, images, tool definitions, tool choice, tool calls, and tool results are
mapped to Converse. Streamed tool-call JSON is buffered per content block so
Claude, OpenAI, and Gemini clients all receive one complete argument object.

OpenAI, Claude, and Gemini Count Tokens converge on Runtime's
`/model/{modelId}/count-tokens` with a native `input.converse` body. The API key
must allow the `bedrock:CountTokens` IAM action. OpenAI Compact is the sole
exception to Converse: AWS does not support compaction through Converse, so it
uses Anthropic compaction through Runtime InvokeModel. Route Compact to a model
that supports it, such as `us.anthropic.claude-sonnet-4-6`.

Claude `cache_control`, OpenAI `prompt_cache_breakpoint`, provider cache rules,
and enabled magic strings become Bedrock Converse `cachePoint` blocks. Bedrock's
cache read/write usage is mapped back to the downstream protocol. GPROXY forwards
Claude `5m`/`1h` TTLs on `cachePoint`; whether a TTL is accepted depends on the
selected Bedrock model, API, and Region. Unsupported or omitted TTLs use that
model's Runtime default.

The channel does not expose embeddings or image operations. Converse also does
not represent every stateful Responses feature; hosted tools, background jobs,
and conversation state are unsupported.

The channel supports provider `cache_breakpoint` rules and magic-string cache
triggers. Anthropic server-side fallback is not available on Amazon Bedrock;
use provider-level routing or client-side fallback instead. Model and API
availability remains region-dependent.

### Claude fallback

Set `settings_json.claude_fable_fallbacks` on `claudeapi`, `claudecode`,
`vercel`, or a Claude-compatible `custom` channel to retry policy refusals from
Claude models. The setting name is retained for compatibility with existing
configurations. Use the string `"default"` for Anthropic's category-aware
default routing, or an ordered array of one to three model IDs. GPROXY adds
`server-side-fallback-2026-07-01` for default routing and
`server-side-fallback-2026-06-01` for an explicit chain. Caller-provided
`fallbacks` remain authoritative. Models known not to accept Anthropic's
server-side `fallbacks` parameter are skipped; unknown and future models remain
eligible by default.

OpenRouter uses the same setting for its own model-routing `fallbacks` array,
not Anthropic's beta. Because OpenRouter has no equivalent to Anthropic's
`"default"` mode, that value maps to an explicit Claude Opus 4.8 fallback.

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
| `region` | AWS region for `aws-bedrock`; defaults to `us-east-1`. |
| `enable_openai_magic_cache` | Recognize GPROXY cache trigger strings on OpenAI Chat/Responses targets and write explicit OpenAI breakpoints. Available for OpenAI, Azure, Amazon Bedrock, Codex, OpenRouter, Vercel, and custom endpoints. |
| `enable_claude_magic_cache` | Recognize GPROXY cache trigger strings on Claude Messages targets and write `cache_control`. Available for Azure, Amazon Bedrock, Claude API, Claude Code, OpenRouter, Vercel, and custom endpoints. |
| `claude_fable_fallbacks` | Retry Claude model refusals with `"default"` Anthropic routing or an ordered array of one to three models. Supported on Claude API-like channels and as an explicit model chain on OpenRouter. |

For external-channel forms, the shared Console controls reserve `base_url`,
`endpoints`, `circuit_breaker`, and `auto_refresh_models`. If external metadata
also declares one of those keys, the shared control owns it and the duplicate
generic field is omitted, so the key is rendered and serialized exactly once.
Those controls honor metadata defaults. Object defaults for `endpoints` and
`circuit_breaker` are initialized only when valid and no persisted value exists.
When marked required, `base_url` must be nonempty, `endpoints` must contain at
least one valid exact URL, and `circuit_breaker` must contain positive integer
values for both `consecutive_failures` and `cooldown_secs` before a provider can
be saved. Unrecognized persisted settings are preserved. Other external setting
declarations are rendered generically; duplicate declarations use the first
field.

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
