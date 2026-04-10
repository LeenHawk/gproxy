---
title: Custom Channels
description: Connect upstream providers compatible with OpenAI, Claude, or Gemini wire formats. No code changes required.
---

Custom channels let you proxy traffic to any upstream that speaks a standard LLM wire protocol. No code changes, no rebuilds -- just config.

## Supported upstream formats

Custom mode works with upstreams compatible with:

- **OpenAI** -- `/v1/chat/completions`, `/v1/responses`, `/v1/models`, `/v1/embeddings`, `/v1/images/generations`
- **Claude** -- `/v1/messages`, `/v1/messages/count_tokens`, `/v1/models`
- **Gemini** -- `/v1beta/models/{model}:generateContent`, `/v1beta/models/{model}:streamGenerateContent`, `/v1beta/models/{model}:countTokens`, `/v1beta/models/{model}:embedContent`

If your upstream uses non-standard signing, custom auth handshakes, or heavily modified request/response schemas, custom mode is not enough. You need a native channel implementation.

## Minimal config

```toml
[[providers]]
name = "my-upstream"
channel = "custom"
settings = { base_url = "https://api.example.com" }
credentials = [{ api_key = "sk-replace-me" }]
```

## Settings

| Field | Default | Description |
|-------|---------|-------------|
| `base_url` | (required) | Upstream base URL |
| `user_agent` | (none) | Custom `User-Agent` header |
| `max_retries_on_429` | `3` | Retry count on 429 rate limit responses |
| `auth_scheme` | `bearer` | Authentication method: `bearer`, `x-api-key`, or `query-key` |

Auth schemes:

- `bearer` -- sends `Authorization: Bearer <api_key>` header
- `x-api-key` -- sends `x-api-key: <api_key>` header
- `query-key` -- appends `?key=<api_key>` to the URL

## Request field stripping with `mask_table`

Strip fields from the request body before forwarding upstream. Useful when your upstream rejects unknown fields.

```toml
[[providers]]
name = "my-upstream"
channel = "custom"

[providers.settings]
base_url = "https://api.example.com"

[providers.settings.mask_table]
rules = [
  { method = "POST", path = "/v1/chat/completions", remove_fields = ["metadata"] },
  { method = "POST", path = "/v1/responses", remove_fields = ["metadata", "previous_response_id"] },
]
```

What `mask_table` can do:

- Match requests by HTTP method and path.
- Remove top-level JSON fields from the request body.

What `mask_table` cannot do:

- Rewrite response bodies.
- Inject custom signing or auth logic.
- Implement arbitrary protocol transformations.

## Custom dispatch rules

By default, the custom channel registers universal passthrough for all operation/protocol combinations. You can override this with explicit dispatch rules to restrict or reroute specific operations.

```toml
[providers.dispatch]
rules = [
  { route = { operation = "ModelList", protocol = "OpenAi" }, implementation = "Passthrough" },
  { route = { operation = "ModelGet", protocol = "OpenAi" }, implementation = "Passthrough" },
  { route = { operation = "GenerateContent", protocol = "OpenAiChatCompletion" }, implementation = "Passthrough" },
  { route = { operation = "StreamGenerateContent", protocol = "OpenAiChatCompletion" }, implementation = "Passthrough" },
  { route = { operation = "CountToken", protocol = "OpenAi" }, implementation = "Local" },
]
```

Dispatch implementations:

| Implementation | Behavior |
|----------------|----------|
| `Passthrough` | Forward request as-is to upstream (same protocol) |
| `TransformTo` | Transform to a different operation/protocol pair before sending |
| `Local` | Handle locally without contacting upstream |
| `Unsupported` | Return 501 |

## Capability boundary

Custom channels can choose dispatch routes but only within GPROXY's existing operation and protocol model. You can select which operations are passthrough, transformed, local, or unsupported. You cannot introduce a new wire protocol or implement bespoke conversion logic.

Available operations: `ModelList`, `ModelGet`, `CountToken`, `GenerateContent`, `StreamGenerateContent`, `Embedding`, `CreateImage`, `StreamCreateImage`, `CreateImageEdit`, `StreamCreateImageEdit`, `Compact`, `OpenAiResponseWebSocket`, `GeminiLive`.

Available protocols: `OpenAi`, `OpenAiResponse`, `OpenAiChatCompletion`, `Claude`, `Gemini`, `GeminiNDJson`.
