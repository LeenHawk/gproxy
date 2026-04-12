---
title: First Request
description: Send your first LLM request through GPROXY using the OpenAI, Claude, or Gemini compatible surface.
---

GPROXY accepts traffic on the standard OpenAI / Anthropic / Gemini HTTP
shapes. Point any client library at your GPROXY base URL, authenticate with
a **user** API key, and you're done.

Before the first request though, you need to understand **how GPROXY picks
the upstream provider**, because there are two different routing modes and
they affect how you write the `model` field.

## Two routing modes

### Aggregated endpoint — `/v1/...`, `/v1beta/...`

One base URL fans out to every provider. The provider must be encoded
**into the model name** using a `provider/model` prefix (or by being
implied by an alias):

```text
POST /v1/chat/completions
{ "model": "openai-main/gpt-4.1-mini", ... }
```

- `openai-main` is the provider name as defined in your config.
- `gpt-4.1-mini` is the upstream model id.
- Gemini-style URI paths work the same way:
  `/v1beta/models/openai-main/gpt-4.1-mini:generateContent`.
- If the `model` you send matches an **alias**, you don't need a prefix —
  the alias already resolves to a `(provider, model)` pair.

### Scoped endpoint — `/{provider}/v1/...`

The provider name is in the URL path, so the `model` field holds the raw
upstream id with no prefix:

```text
POST /openai-main/v1/chat/completions
{ "model": "gpt-4.1-mini", ... }
```

This form is the closest thing to "pretend GPROXY is the upstream directly"
and is the simplest drop-in for clients that hard-code a base URL and model
name together.

:::tip
Pick whichever fits your client. A single GPROXY instance serves **both**
forms simultaneously — they coexist for every protocol (OpenAI / Claude /
Gemini).
:::

## Examples

All examples below assume:

- GPROXY is listening on `http://127.0.0.1:8787`
- Admin key `sk-admin-1` (from the [Quick Start](/getting-started/quick-start/))
- Provider `openai-main` exposes `gpt-4.1-mini`

### OpenAI Chat Completions

Aggregated `/v1`:

```bash
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer sk-admin-1" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "openai-main/gpt-4.1-mini",
    "messages": [
      { "role": "user", "content": "Say hello in one short sentence." }
    ]
  }'
```

Scoped `/openai-main/v1`:

```bash
curl http://127.0.0.1:8787/openai-main/v1/chat/completions \
  -H "Authorization: Bearer sk-admin-1" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4.1-mini",
    "messages": [
      { "role": "user", "content": "Say hello in one short sentence." }
    ]
  }'
```

If you've defined an alias (for example `chat-default`), you can use it on
either endpoint without a prefix — the alias resolves to a concrete
`(provider, model)` pair internally:

```json
{ "model": "chat-default", "messages": [ ... ] }
```

The non-stream response has its `"model"` field rewritten back to the
alias the client sent; streaming chunks are rewritten per chunk inside
the engine.

### Anthropic Messages

```bash
curl http://127.0.0.1:8787/v1/messages \
  -H "x-api-key: sk-admin-1" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "anthropic-main/claude-3-5-sonnet-latest",
    "max_tokens": 256,
    "messages": [ { "role": "user", "content": "Hello" } ]
  }'
```

Or, using the scoped form:

```bash
curl http://127.0.0.1:8787/anthropic-main/v1/messages \
  -H "x-api-key: sk-admin-1" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-5-sonnet-latest",
    "max_tokens": 256,
    "messages": [ { "role": "user", "content": "Hello" } ]
  }'
```

### Gemini generateContent

Aggregated — the provider prefix is embedded into the `models/...` path
segment:

```bash
curl "http://127.0.0.1:8787/v1beta/models/vertex-main/gemini-1.5-flash:generateContent" \
  -H "x-goog-api-key: sk-admin-1" \
  -H "Content-Type: application/json" \
  -d '{ "contents": [ { "parts": [ { "text": "Hello" } ] } ] }'
```

Scoped:

```bash
curl "http://127.0.0.1:8787/vertex-main/v1beta/models/gemini-1.5-flash:generateContent" \
  -H "x-goog-api-key: sk-admin-1" \
  -H "Content-Type: application/json" \
  -d '{ "contents": [ { "parts": [ { "text": "Hello" } ] } ] }'
```

## Listing models

Both endpoints serve the standard list route:

```bash
# Aggregated — returns models from every provider the user can see.
curl http://127.0.0.1:8787/v1/models \
  -H "Authorization: Bearer sk-admin-1"

# Scoped — returns only models on openai-main.
curl http://127.0.0.1:8787/openai-main/v1/models \
  -H "Authorization: Bearer sk-admin-1"
```

Aliases show up as first-class entries alongside real models, filtered by
the requesting user's permissions. `GET /v1/models/{id}` resolves
individual entries (including aliases).

## What gets logged

If `enable_usage = true` (see the [TOML Config reference](/reference/toml-config/))
GPROXY records per-request usage — tokens, cost, user, provider, model —
asynchronously through the `UsageSink` worker. You can inspect it from
the console or query the admin API.

If `enable_upstream_log` or `enable_downstream_log` are on, the request
and response envelopes are captured too; body capture is a separate flag
so you can keep the metadata lightweight in production.

## Troubleshooting

- **`401 unauthorized`** — The API key is missing, unknown, or disabled.
- **`400` / `provider prefix`** — You hit `/v1` without a `provider/` prefix
  and the `model` didn't match an alias. Either add the prefix, use a
  scoped path, or define an alias for that name.
- **`403 forbidden: model`** — The user has no permission matching the
  requested model. Check `[[permissions]]` or the console's *Permissions*
  tab.
- **`429 rate_limited`** — A user/model rate limit kicked in. See
  [Permissions, Rate Limits & Quotas](/guides/permissions/).
- **`402 quota_exceeded`** — The user's cost quota is spent. Top it up
  from the console or the admin API.
