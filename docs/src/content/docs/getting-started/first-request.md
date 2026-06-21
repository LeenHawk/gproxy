---
title: First Request
description: Send OpenAI, Claude, and Gemini compatible requests through GPROXY v2.
---

GPROXY v2 exposes OpenAI, Anthropic, and Gemini-compatible HTTP surfaces. A user
API key authenticates the caller; providers, routes, route members, aliases,
rules, permissions, quotas, and credentials decide where the request goes.

The two routing modes matter before you write the `model` field.

## Aggregated Routing

Aggregated routes live at `/v1/*` and `/v1beta/*`. In this mode the requested
model name resolves through v2 route data:

```text
client model -> alias or route name -> route member -> provider credential
```

For the quick-start bundle, the route name is `main`:

```bash
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer sk-dev-local" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "main",
    "messages": [
      { "role": "user", "content": "Say hello." }
    ]
  }'
```

Aggregated model list routes are served by GPROXY's own snapshot:

```bash
curl http://127.0.0.1:8787/v1/models \
  -H "Authorization: Bearer sk-dev-local"
```

The list contains route and alias names the key is allowed to see, not a raw
dump of every upstream provider model.

## Scoped Provider Routing

Scoped routes live at `/{provider}/v1/*` and `/{provider}/v1beta/*`. In this
mode the provider comes from the URL and the model is the upstream model id:

```bash
curl http://127.0.0.1:8787/openai-main/v1/chat/completions \
  -H "Authorization: Bearer sk-dev-local" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4.1-mini",
    "messages": [
      { "role": "user", "content": "Say hello." }
    ]
  }'
```

Scoped mode bypasses route balancing. It still authenticates the user key,
checks permission against the provider name, applies provider rules, selects a
credential, rewrites compatible request bodies, logs according to instance
settings, and settles usage.

## Claude Messages

Claude-compatible calls use the Anthropic message shape. If the request is
aggregated, use a route or alias name:

```bash
curl http://127.0.0.1:8787/v1/messages \
  -H "x-api-key: sk-dev-local" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "main",
    "max_tokens": 256,
    "messages": [
      { "role": "user", "content": "Hello from Claude format." }
    ]
  }'
```

For scoped mode, place the provider in the path and use the upstream model id:

```bash
curl http://127.0.0.1:8787/claude-main/v1/messages \
  -H "x-api-key: sk-dev-local" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4-5",
    "max_tokens": 256,
    "messages": [
      { "role": "user", "content": "Hello from Claude format." }
    ]
  }'
```

The exact model id must exist upstream or be accepted by that provider. v2 keeps
scoped model validation intentionally lax and lets the provider answer.

## Gemini generateContent

Gemini carries the model in the path. Aggregated requests use a route or alias
inside the `models/...` segment:

```bash
curl "http://127.0.0.1:8787/v1beta/models/main:generateContent" \
  -H "x-goog-api-key: sk-dev-local" \
  -H "Content-Type: application/json" \
  -d '{
    "contents": [
      { "parts": [ { "text": "Hello from Gemini format." } ] }
    ]
  }'
```

Scoped requests use the provider path prefix. The example below assumes you
have created a Gemini-capable provider named `aistudio-main`:

```bash
curl "http://127.0.0.1:8787/aistudio-main/v1beta/models/gemini-1.5-flash:generateContent" \
  -H "x-goog-api-key: sk-dev-local" \
  -H "Content-Type: application/json" \
  -d '{
    "contents": [
      { "parts": [ { "text": "Hello from Gemini format." } ] }
    ]
  }'
```

## Common Errors

| Symptom | Meaning |
| --- | --- |
| `401` | Missing, unknown, or disabled user API key. |
| `403` | The key is valid but lacks permission for the route or provider name. |
| `404` or unknown route | Aggregated mode could not resolve the requested route or alias. |
| `429` | Rate limit exceeded. |
| `402` | Quota precheck failed. |
| `413` | Request body exceeded the shared native/edge body limit. |

Every gateway response includes `x-gproxy-request-id` for correlation with logs.
