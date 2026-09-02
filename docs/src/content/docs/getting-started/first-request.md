---
title: First Request
description: Send OpenAI Chat, OpenAI Responses, Claude Messages, and Gemini requests through GPROXY, stream them, list models, use the named prefix, and find the request afterwards.
---

GPROXY answers on the native paths of each accepted wire format. A user API key
authenticates the caller; the public model name selects the load balancer, and
its members, permissions, quotas, rules, and credentials decide where the
request goes. The examples assume a public model name `main` and a key
`sk-<your-key>` created as in the [Quick Start](/getting-started/quick-start/).

## Authentication

Send the key in any of these headers, on any path:

```text
Authorization: Bearer sk-<your-key>
x-api-key: sk-<your-key>
x-goog-api-key: sk-<your-key>
```

The key must be enabled and unexpired, and it needs an allow permission for
the provider the request resolves to.

## OpenAI Chat Completions

```bash
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer sk-<your-key>" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "main",
    "messages": [
      { "role": "user", "content": "Say hello." }
    ]
  }'
```

## OpenAI Responses

```bash
curl http://127.0.0.1:8787/v1/responses \
  -H "Authorization: Bearer sk-<your-key>" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "main",
    "input": "Say hello."
  }'
```

## Claude Messages

```bash
curl http://127.0.0.1:8787/v1/messages \
  -H "x-api-key: sk-<your-key>" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "main",
    "max_tokens": 256,
    "messages": [
      { "role": "user", "content": "Say hello." }
    ]
  }'
```

## Gemini GenerateContent

Gemini carries the model in the path:

```bash
curl "http://127.0.0.1:8787/v1beta/models/main:generateContent" \
  -H "x-goog-api-key: sk-<your-key>" \
  -H "Content-Type: application/json" \
  -d '{
    "contents": [
      { "parts": [ { "text": "Say hello." } ] }
    ]
  }'
```

If the load balancer's member speaks another format, GPROXY converts the
request on the way out and the response on the way back. The client sees its
own format in every case.

## Streaming

For OpenAI Chat, OpenAI Responses, and Claude Messages, add `"stream": true`
to the body; the response is server-sent events in that format's own event
shape:

```bash
curl -N http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer sk-<your-key>" \
  -H "Content-Type: application/json" \
  -d '{ "model": "main", "stream": true,
        "messages": [ { "role": "user", "content": "Count to five." } ] }'
```

For Gemini, call `:streamGenerateContent`. Without a query the stream is
Gemini's incremental JSON array; `?alt=sse` selects server-sent events:

```bash
curl -N "http://127.0.0.1:8787/v1beta/models/main:streamGenerateContent?alt=sse" \
  -H "x-goog-api-key: sk-<your-key>" \
  -H "Content-Type: application/json" \
  -d '{ "contents": [ { "parts": [ { "text": "Count to five." } ] } ] }'
```

OpenAI Responses is also served over WebSocket: an upgrade request to
`GET /v1/responses` opens a session that goes through the same admission and
settlement as an HTTP call.

## Listing Models

```bash
curl http://127.0.0.1:8787/v1/models \
  -H "Authorization: Bearer sk-<your-key>"
```

`GET /v1/models` answers in the OpenAI or Claude shape, `GET /v1beta/models`
in the Gemini shape, and `GET /v1/models/{id}` returns one entry. The list
contains the public model names and variants the key may use. Providers whose
**Refresh the model list from upstream** setting is on (the default) are asked
for their catalogue concurrently. Listing is answered by the gateway itself,
still passes admission, and records a zero-cost settlement.

## The Named Prefix

Putting a target name in the first path segment selects it directly, and the
rest of the path is the native path:

```bash
curl http://127.0.0.1:8787/openai-main/v1/chat/completions \
  -H "Authorization: Bearer sk-<your-key>" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4.1-mini",
    "messages": [ { "role": "user", "content": "Say hello." } ]
  }'
```

| First segment | Effect on `model` |
| --- | --- |
| A provider name, such as `openai-main` | `model` is the upstream model id. The provider's credential pool is used directly; load-balancer selection is skipped. |
| A route name | The route's members are used as in aggregated mode. |
| A namespace, such as `openai` for a public model `openai/gpt-4.1` | `model` is the part after the slash. |

Named requests still authenticate the key, check permissions against the
selected provider, apply its rule sets, select a credential, and settle usage.
A first segment that matches nothing is treated as part of an aggregated path.
Codex CLI uses this form: the portal's connection snippet points it at
`/codex/backend-api/...` and `/codex/oauth/...`.

## Errors

| Status | Meaning |
| --- | --- |
| `400` | The path or operation is not supported, or the request body is invalid. |
| `401` | The key is missing, unknown, disabled, or expired. |
| `402` | A cost quota is exhausted. |
| `403` | The key has no allow permission for the resolved provider, or a deny applies. |
| `404` | The public model name, route, or provider does not exist. |
| `413` | The request body exceeds the 100 MiB limit. |
| `429` | A rate limit was hit; `retry_after_secs` is in the body. |
| `502` | No usable credential, or every upstream attempt failed. |

Error bodies use the OpenAI envelope: `{"error":{"message":"..."}}`.

## Finding the Request Afterwards

Every response carries an `x-request-id` header of the form
`<instance-id>-<random>-<sequence>`. In the console:

- **Statistics → Usage** shows requests, input and output tokens, cache reads
  and writes, and settled cost, filtered by provider, credential, user, key,
  or model.
- **Statistics → Request audit** lists each client request with every
  upstream call it produced, filtered by user, key, provider, status, or
  request id. Headers and bodies appear only when the corresponding capture
  switches are on in **Settings**, and are redacted unless redaction is
  disabled there.
- **Statistics → Admin actions** records console changes and channel sign-ins.

Users who sign in to `/portal` see their own usage and, when the operator has
enabled **Show recent settled requests** in **Settings → User portal**, a
**Recent settled requests** table with provider, operation, upstream model,
tokens, cost, and latency — never request or response bodies.
