---
title: "Routing & Endpoints"
description: "Ingress modes, every operation and its paths per wire family, model selection and resolution, the failover budget, and the headers stripped before egress"
---

Every request that reaches the gateway is classified against one
operation registry (`crates/gproxy-protocol/src/specs/`), resolved to an
ordered list of upstream candidates, and sent through a single settlement
funnel. This page is the reference for each of those steps.

## Aggregated and Named Ingress

GPROXY answers two shapes of path.

| Mode | Path shape | What selects the backend |
| --- | --- | --- |
| Aggregated | A declared operation path: `/v1/chat/completions`, `/v1/messages`, `/v1beta/models/{model}:generateContent`, ... | The model name in the request, through aliases and routes. |
| Named | `/{name}/...` where the remainder is a declared operation path or a channel service surface: `/codex/v1/responses`, `/codex/backend-api/codex/responses`, `/codex/oauth/token` | The first path segment. |

A first segment counts as a name when it is one of: a **route namespace**
(the part before `/` in an exposed model name such as `openai/gpt-5`,
matched case-insensitively), a **route name**, or a **provider name**,
checked in that order. If the segment is not a known name, or the
remainder is not a declared path, the whole path is treated as aggregated
and fails classification with 400.

- Named to a namespace: the request's model is looked up inside that
  namespace, so `/{ns}/v1/...` with `"model": "gpt-5"` resolves
  `ns/gpt-5`.
- Named to a route: the route's members serve the request regardless of
  the model in the body; each member's `upstream_model` is what goes
  upstream.
- Named to a provider: routes are bypassed. The plan is every enabled
  credential of that provider, and the requested model name is sent
  upstream as-is after provider-scoped aliases and variants. The models
  list follows the provider's routing rule instead of being answered
  from the snapshot.

Service surfaces (Codex CLI's `backend-api` routes, Claude Code's
control-plane routes) are only reachable in named mode, because a channel
declares them rather than the operation registry. They pass the same
authentication, admission and settlement as any other request.

## How a Client Names the Model

| Wire family | Model comes from | Family detection |
| --- | --- | --- |
| OpenAI Chat, Responses, and every other `/v1/...` operation | body `model`; realtime calls also accept `session.model` | Path. |
| Claude Messages | body `model` | Path; on shared paths (`/v1/models`, `/v1/models/{id}`) any of `x-api-key`, `anthropic-version`, `anthropic-beta` marks the caller as Claude. |
| Gemini | the `{model}` path segment, `/v1beta/models/{model}:generateContent` | Path. |
| Model get | the `{id}` or `{model}` path segment | Path. |

Streaming is detected per ingress: a boolean `stream` in the body (Chat,
Responses, Messages, image generation), `stream` in JSON or a multipart
field (image edits, transcription), `stream_format: "sse"` (speech), or
the endpoint itself (`:streamGenerateContent`, guardian, WebSocket
upgrades). A `stream: true` body promotes `generate_content` to
`stream_generate_content`. Gemini streams are an incremental JSON array
unless the request carries `?alt=sse`.

A routing rule may change stream-ness alone by targeting the sibling
operation. `generate_content` → `stream_generate_content` forces a streaming
upstream and collapses the events into one object for the non-stream client
(how event-stream-only upstreams such as Kiro are served). `stream_generate_content` →
`generate_content` fetches one object from the upstream and synthesizes the
client's stream from it. On native hosts that stream opens immediately and
carries a keepalive every 15 seconds while the upstream works (Claude
`ping`, an SSE comment, or JSON whitespace for Gemini arrays); an upstream
failure after that point arrives as the protocol's error event. Edge hosts
buffer and synthesize at the end instead.

## Operations

Settle modes: **response** is billed from the response or stream tail;
**free** is never billed; **session end** is billed when a realtime
session closes; **completed status** is billed once, when a polled video
reports `completed`. Affinity **session** pins a conversation to a
credential; **resource** pins follow-up calls to the credential that
created the file, video, character or call. Family ids are `openai`,
`claude`, `gemini`; content-generation kinds are `openai_chat`,
`openai_responses`, `openai_responses_websocket`, `claude_messages`,
`gemini_generate_content`.

### Models and Tokens

| Operation | Method and path (family) | Settle | Affinity |
| --- | --- | --- | --- |
| `list_models` | `GET /v1/models` (openai, claude); `GET /v1beta/models` (gemini) | free | — |
| `get_model` | `GET /v1/models/{id}` (openai, claude); `GET /v1beta/models/{model}` (gemini) | free | — |
| `count_tokens` | `POST /v1/messages/count_tokens` (claude); `POST /v1/responses/input_tokens` (openai); `POST /v1beta/models/{model}:countTokens` (gemini) | free | — |

### Generate Content, Compact and Memories

| Operation | Method and path (kind) | Settle | Affinity |
| --- | --- | --- | --- |
| `generate_content` | `POST /v1/chat/completions` (openai_chat); `POST /v1/responses` (openai_responses); `POST /v1/messages` (claude_messages); `POST /v1beta/models/{model}:generateContent` (gemini_generate_content) | response | session |
| `stream_generate_content` | the four above with `stream: true`; `POST /v1beta/models/{model}:streamGenerateContent`; `GET /v1/responses` WebSocket upgrade (openai_responses_websocket) | response | session |
| `guardian_review` | `POST /v1/guardian` (openai_responses, always streams) | response | session |
| `guardian_classify` | `POST /v1/guardian-classifier` (openai_responses, always streams) | response | session |
| `compact_content` | `POST /v1/responses/compact` (openai) | response | session |
| `summarize_memory` | `POST /v1/memories/trace_summarize` (openai) | response | — |

### Embeddings, Rerank and Search

| Operation | Method and path (family) | Settle | Affinity |
| --- | --- | --- | --- |
| `create_embedding` | `POST /v1/embeddings` (openai); `POST /v1beta/models/{model}:embedContent` (gemini) | response | — |
| `batch_create_embedding` | `POST /v1beta/models/{model}:batchEmbedContents` (gemini) | response | — |
| `rerank` | `POST /v1/rerank` (openai) | response | — |
| `web_search` | `POST /v1/alpha/search` (openai) | response | — |

### Images and Audio

| Operation | Method and path (family) | Settle | Affinity |
| --- | --- | --- | --- |
| `create_image` | `POST /v1/images/generations` (openai, `stream` flag); `POST /v1beta/models/{model}:predict` (gemini) | response | — |
| `edit_image` | `POST /v1/images/edits` (openai, `stream` in JSON or multipart) | response | — |
| `create_speech` | `POST /v1/audio/speech` (openai, streams when `stream_format` is `sse`) | response | — |
| `create_transcription` | `POST /v1/audio/transcriptions` (openai, `stream` in JSON or multipart) | response | — |
| `create_translation` | `POST /v1/audio/translations` (openai) | response | — |

### Files

| Operation | Method and path (family) | Settle | Affinity |
| --- | --- | --- | --- |
| `create_file` | `POST /v1/files` (openai); `POST /upload/v1beta/files` (gemini) | free | resource `file` |
| `list_files` | `GET /v1/files` (openai); `GET /v1beta/files` (gemini) | free | resource `file` |
| `retrieve_file` | `GET /v1/files/{id}` (openai); `GET /v1beta/files/{id}` (gemini) | free | resource `file` |
| `retrieve_file_content` | `GET /v1/files/{id}/content` (openai); `GET /v1beta/files/{id}:download` and `GET /download/v1beta/files/{id}:download` (gemini) | free | resource `file` |
| `delete_file` | `DELETE /v1/files/{id}` (openai); `DELETE /v1beta/files/{id}` (gemini) | free | resource `file` |

### Video

| Operation | Method and path (family) | Settle | Affinity |
| --- | --- | --- | --- |
| `create_video` | `POST /v1/videos` (openai); `POST /v1beta/models/{model}:predictLongRunning` (gemini) | free | resource `video` |
| `retrieve_video` | `GET /v1/videos/{id}` (openai); `GET /v1beta/operations/{id}` and `GET /v1beta/models/{model}/operations/{id}` (gemini) | completed status | resource `video` |
| `list_videos` | `GET /v1/videos` (openai) | free | resource `video` |
| `delete_video` | `DELETE /v1/videos/{id}` (openai) | free | resource `video` |
| `download_video_content` | `GET /v1/videos/{id}/content` (openai) | free | resource `video` |
| `remix_video` | `POST /v1/videos/{id}/remix` (openai) | free | resource `video` |
| `edit_video` | `POST /v1/videos/edits` (openai) | free | resource `video` |
| `extend_video` | `POST /v1/videos/extensions` (openai) | free | resource `video` |
| `create_video_character` | `POST /v1/videos/characters` (openai) | free | resource `video_character` |
| `get_video_character` | `GET /v1/videos/characters/{id}` (openai) | free | resource `video_character` |

A video job is billed when a poll of `retrieve_video` first reports
`completed`; the settlement is de-duplicated across polls, so creation and
later polls carry no cost of their own.

### Realtime

| Operation | Method and path (family) | Settle | Affinity |
| --- | --- | --- | --- |
| `create_realtime_call` | `POST /v1/realtime/calls` (openai) | session end | resource `realtime_call` |
| `connect_realtime` | `GET /v1/realtime` WebSocket upgrade (openai) | session end | session |

## Answered Locally

`list_models` and `get_model` in aggregated or namespace mode are answered
from the control-plane snapshot: the exposed models plus, for providers
with `auto_refresh_models` on, a concurrent upstream catalogue fetch
merged into the list. `count_tokens`, and any other cell whose routing
rule (or channel default) is `local`, is answered by the gateway with its
tokenizer ladder. Local answers still authenticate, pass permission and
rate-limit checks, and complete admission and request telemetry. Free
operations never pre-charge a quota and write no usage row.

## WebSocket Ingresses

Upgrade intent is detected from `sec-websocket-*`, `Upgrade: websocket`
or `Connection: upgrade` headers. Two operations accept it:
`GET /v1/responses` (Responses over WebSocket, an envelope of the
Responses wire shape that composes onto the Responses transform pairs)
and `GET /v1/realtime`. Channels may add WebSocket service surfaces of
their own, reachable in named mode. A path that matches an upgrade
ingress without an upgrade, or the reverse, is rejected. Upgrades work on
the native host and on Cloudflare and Deno; Netlify answers 501.

## Resolution Order

For a request that names a model, the control plane resolves in this
order. The order is exercised by
`crates/gproxy-core/src/tests/pricing.rs`.

1. **Alias.** A global alias (an `aliases` row with no provider) is applied
   first; then, when the mode names a provider, that provider's alias.
   Aliases match exactly; the first enabled row by `(priority, id)` wins.
2. **Variant suffix.** If the aliased name is a declared variant of an
   exposed model (`variants_json`), the presets are stripped from the end
   and written into the body: `-thinking-none|low|medium|high|xhigh|adaptive`
   becomes `reasoning_effort` (Chat), `reasoning.effort` (Responses),
   `thinking` (Claude; budgets 1,024 / 10,240 / 32,768 tokens, `adaptive`,
   or `disabled`), or `generationConfig.thinkingConfig.thinkingLevel`
   (Gemini); `-tier-priority|default|scale|flex|auto` and `-fast` become
   `service_tier`. Presets stack (`-thinking-high-tier-flex`). A suffix on
   a name that is not a declared variant is left alone and looked up
   verbatim.
3. **Route.** The resolved name is looked up in `exposed_models`, or as
   `ns/name` inside a namespace. An unknown name is 404.
4. **Members to plan.** Members whose credential is marked dead for this
   model (or for `*`) are dropped. The rest sort by tier, then health
   (healthy before degraded), then member weight, then credential weight.
   Inside the first tier a per-route counter picks the member in
   proportion to its weight, then a credential inside that member: the
   provider strategy `round_robin` advances a per-member counter,
   `sticky` hashes the session or API-key affinity to a stable slot. The
   pick moves to the front; the other members stay in sorted order as
   failover targets. Rotation is a counter, never random, so successive
   requests walk the weighted slots in order.
5. **Credential.** Each target is `(provider, credential, upstream model)`.
   Provider settings, the proxy (credential over provider over global)
   and the TLS fingerprint override travel with it.

## Failover Budget

The budget is `min(route.max_attempts, GPROXY_MAX_ATTEMPTS)`; for a named
provider it is `min(number of credentials, GPROXY_MAX_ATTEMPTS)`. Only
sends count against it. A target is skipped without spending budget when:

- its credential was already marked dead during this request (secret
  rejected, refresh failed, or an upstream `CredentialDead` disposition);
- a resource affinity pins the request to another credential of that
  provider;
- the provider's routing rule for this `(operation, kind)` is
  `unsupported`, or asks for a transform pair that does not exist;
- the credential's own RPM or TPM limit for the current minute is reached.

A sent attempt whose disposition is `Retryable` or `CredentialDead` moves
to the next target; `Success` and `Terminal` are returned to the client.
When no attempt could be sent, the response is 500 (no target supports
the operation), 400 (unsupported), 429 (every credential rate-limited) or
502 (no credentials); when all attempts failed it is 502
`all upstream attempts failed`. Every response carries `x-request-id`,
which ties the attempts together in the request audit.

## Headers Stripped Before Egress

At ingress, before any channel sees the request, GPROXY removes
`x-gproxy-session-id` (its own session-affinity hint), the hop-by-hop
headers (`connection`, `content-length`, `keep-alive`,
`proxy-authenticate`, `proxy-authorization`, `proxy-connection`, `te`,
`trailer`, `transfer-encoding`, `upgrade`, plus anything nominated by
`Connection`), and the caller's credential and forwarding headers
(`accept-encoding`, `api-key`, `authorization`, `cookie`, `forwarded`,
`host`, `via`, `x-api-key`, `x-forwarded-for`, `x-forwarded-host`,
`x-forwarded-proto`, `x-goog-api-key`, `x-real-ip`). The query parameters
`access_token`, `api_key`, `key` and `x-api-key` are removed.

At egress the upstream request is rebuilt from an allow-list: `accept`
and `content-type`, the names the channel declares, and the provider's
`traffic_policy` setting. Names or `prefix-*` patterns in the instance's
global metadata blacklist are removed even when allowed. Query parameters
follow the channel's allow-list the same way. The channel then adds its
own authentication. Response headers to the client are filtered alike: a
base list (`accept-ranges`, `allow`, `cache-control`,
`content-disposition`, `content-encoding`, `content-range`,
`content-type`, `etag`, `expires`, `last-modified`, `link`, `location`,
`retry-after`, `vary`) plus channel additions, and never `alt-svc`,
`server`, `set-cookie`, `set-cookie2`, `via` or `www-authenticate`.

## Routing Rules

Whether a provider's cell is passthrough, transformed to another kind,
answered locally or unsupported is a per-provider matrix keyed by
`(operation, kind)`, seeded from the channel's defaults and editable in
the Rules workspace. See [Routing Rules & Rule Sets](/guides/rules/).
