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
| `openrouter`, `deepseek`, `groq`, `nvidia`, `vercel` | API-key providers with OpenAI-like surfaces. |
| `claudeapi` | Anthropic Claude Messages API. |
| `aistudio`, `vertex`, `vertexexpress` | Gemini / Vertex upstreams; `vertex` also supports native Claude partner models. |
| `codex`, `claudecode`, `geminicli`, `antigravity`, `grokbuild`, `kiro`, `copilotcli` | OAuth, device-code, cookie, or envelope-style agent channels. |
| `chatgpt` | ChatGPT consumer web backend via a chatgpt.com session cookie. |
| `claudeweb` | Claude consumer web backend via a claude.ai session cookie (native only). |
| `tasklet` | Tasklet Agent API via a browser session token (native only). |

Every channel declares a routing surface as `(Operation, OperationKind) ->
RoutingDecision`. That is the source for the provider's default
`routing_rules` rows. Request behavior is therefore described by operation
capability, not by provider-family buckets.

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

### Tasklet channel (session token)

The native-only `tasklet` channel submits each generation as a new Tasklet Agent,
then follows its `thinking`, autonomous tool execution, and final content over the
Tasklet sync WebSocket. It accepts OpenAI Chat, Responses, Claude Messages, and
Gemini generation requests through the normal routing transforms. Inline base64
images/files are uploaded first; Tasklet `f_...` file ids can be passed directly.

Create a manual credential with `session_token` and `workspace_id`. Both values
come from an authenticated tasklet.ai browser session and are password-equivalent;
do not commit or share them. Optional provider settings are `timezone` (default
`UTC`) and `emit_tool_trace` (default `false`, emits tool names as reasoning).

To obtain the values:

1. Sign in to Tasklet, open the browser developer tools, select **Network**, and
   send any message to an agent. Reload the page first if the request is not
   captured.
2. Open the `POST https://api.tasklet.ai/api/sendChatMessage` request.
3. Copy the value after `Bearer ` in the `Authorization` request header into
   `session_token`. Do not include the `Bearer ` prefix.
4. Copy `workspaceId` from the JSON request payload into `workspace_id`.

The token can also be seen in the first `connect` message sent over the
`/api/sync` WebSocket as `sessionToken`. Treat it like a password and replace it
in gproxy after the Tasklet session is revoked or expires.

The `channel-tasklet` feature also embeds a Rust MCP server for client-side tool
calls. Expose gproxy over public HTTPS and create a dedicated gproxy user API key.
In the same Tasklet workspace, connect
`https://YOUR_GPROXY_HOST/tasklet/mcp` as an MCP server, open **Advanced →
Headers**, and add `X-API-Key: YOUR_GPROXY_USER_KEY`. Approve its
`gproxy_call_client_tool` tool. This connection is made once. The MCP endpoint
does not accept keys in its query string.

When an OpenAI-compatible request contains function or custom tools, gproxy gives
Tasklet their schemas and a short-lived, single-use turn id. A Tasklet MCP call
is then returned to the original caller as a normal `tool_calls` response;
gproxy does not execute that client tool.

The MCP endpoint requires an enabled gproxy user key and exposes no credentials
or active tool catalogue. Delegation additionally requires the unguessable turn
id carried only by the active Tasklet turn. Revoke the dedicated user key to
disable Tasklet MCP access. In a multi-instance deployment, route the generation
and its MCP callback to the same gproxy process, because active response streams
are process-local.

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
| `enable_magic_cache` | Recognize GPROXY cache trigger strings and write native Claude or OpenAI cache breakpoints. Available for OpenAI, Codex, Claude API, Claude Code, OpenRouter, and Vercel. |
| `enable_claude_fable_fallback` | Add the supported Claude Fable-to-Opus fallback behavior on Claude-capable channels. |

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
