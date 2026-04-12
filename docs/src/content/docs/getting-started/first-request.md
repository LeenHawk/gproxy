---
title: First Request
description: Send your first LLM request through gproxy using the OpenAI, Claude, or Gemini compatible surface.
---

gproxy accepts traffic on the standard OpenAI / Anthropic / Gemini HTTP
shapes. Point any client library at your gproxy base URL, authenticate with
a **user** API key, and you're done.

All examples below assume:

- gproxy is listening on `http://127.0.0.1:8787`
- User `alice` has the API key `sk-user-alice-1`
- Provider `openai-main` exposes `gpt-4.1-mini` (and the alias `chat-default`)

## OpenAI-compatible: Chat Completions

```bash
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer sk-user-alice-1" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4.1-mini",
    "messages": [
      { "role": "user", "content": "Say hello in one short sentence." }
    ]
  }'
```

Or target the alias instead of the raw model id:

```json
{ "model": "chat-default", "messages": [ … ] }
```

When you use an alias, non-streaming responses have their `"model"` field
rewritten back to the alias you sent, and streaming chunks are rewritten per
chunk inside the engine. Clients see a consistent name end-to-end.

## Anthropic-compatible: Messages

```bash
curl http://127.0.0.1:8787/v1/messages \
  -H "x-api-key: sk-user-alice-1" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-5-sonnet-latest",
    "max_tokens": 256,
    "messages": [
      { "role": "user", "content": "Hello" }
    ]
  }'
```

This request will route to whichever Anthropic-capable provider and model
the user has permission for. If the upstream speaks a different protocol,
gproxy translates via the protocol `transform` layer.

## Gemini-compatible: generateContent

```bash
curl "http://127.0.0.1:8787/v1beta/models/gemini-1.5-flash:generateContent" \
  -H "x-goog-api-key: sk-user-alice-1" \
  -H "Content-Type: application/json" \
  -d '{
    "contents": [
      { "parts": [ { "text": "Hello" } ] }
    ]
  }'
```

## Listing models

All three protocols have a model-list endpoint:

```bash
curl http://127.0.0.1:8787/v1/models \
  -H "Authorization: Bearer sk-user-alice-1"
```

The response contains both real models and aliases as first-class entries,
filtered by the requesting user's permissions. `GET /v1/models/{id}`
resolves an individual entry (including aliases).

## What gets logged

If `enable_usage = true` (see the [TOML Config reference](/reference/toml-config/))
gproxy records per-request usage — tokens, cost, user, provider, model —
asynchronously through the `UsageSink` worker. You can inspect it from the
console or query the admin API.

If `enable_upstream_log` or `enable_downstream_log` are on, the request and
response envelopes are captured too; body capture is a separate flag so you
can keep the metadata lightweight in production.

## Troubleshooting

- **`401 unauthorized`** — The API key is missing, unknown, or disabled.
- **`403 forbidden: model`** — The user has no permission matching the
  requested model. Check `[[permissions]]` or the console's *Permissions*
  tab.
- **`429 rate_limited`** — A user/model rate limit kicked in. See
  [Permissions, Rate Limits & Quotas](/guides/permissions/).
- **`402 quota_exceeded`** — The user's cost quota is spent. Top it up
  from the console or the admin API.
