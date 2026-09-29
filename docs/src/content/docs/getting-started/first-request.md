---
title: "First request"
description: "Call the gateway through OpenAI, Claude, and Gemini APIs, use streaming, and diagnose common errors."
---

After [Quick start](/getting-started/quick-start/), you should have a model route named `fast` and a gateway API key. These examples use the default address; adjust it if you changed the port.

```sh
export GPROXY_KEY='your-gproxy-api-key'
```

The gateway accepts `Authorization: Bearer <key>`, `x-api-key`, and `x-goog-api-key`. Use a gateway key, not an upstream credential.

## OpenAI Chat Completions

```sh
curl -sS http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"fast","messages":[{"role":"user","content":"Say hello."}]}'
```

## OpenAI Responses

```sh
curl -sS http://127.0.0.1:8787/v1/responses \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"fast","input":"Say hello."}'
```

## Claude Messages

```sh
curl -sS http://127.0.0.1:8787/v1/messages \
  -H "x-api-key: $GPROXY_KEY" \
  -H 'anthropic-version: 2023-06-01' \
  -H 'content-type: application/json' \
  -d '{"model":"fast","max_tokens":256,
       "messages":[{"role":"user","content":"Say hello."}]}'
```

## Gemini GenerateContent

```sh
curl -sS "http://127.0.0.1:8787/v1beta/models/fast:generateContent" \
  -H "x-goog-api-key: $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"contents":[{"parts":[{"text":"Say hello."}]}]}'
```

## Streaming

```sh
curl -sSN http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"fast","stream":true,
       "messages":[{"role":"user","content":"Count to three."}]}'
```

## Gemini streaming

```sh
curl -sSN "http://127.0.0.1:8787/v1beta/models/fast:streamGenerateContent?alt=sse" \
  -H "x-goog-api-key: $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"contents":[{"parts":[{"text":"Count to three."}]}]}'
```

Responses use the protocol requested by the client. Success depends on route members, upstream capabilities, and conversion support. Changing the endpoint does not add capabilities to the upstream.

## Model lists

Use the console's model catalog to find configured gateway names. To query the caller-visible gateway catalog through the API:

```sh
curl -sS http://127.0.0.1:8787/portal/api/models \
  -H "Authorization: Bearer $GPROXY_KEY"
```

`permitted` indicates whether the caller can use an entry. `GET /v1/models` forwards the selected upstream's model list, which is different from the gateway catalog. Application does not expose `/portal/api` over HTTP; use its in-app catalog.

## Select a provider or namespace

| URL | Model in request | Target |
| --- | --- | --- |
| `/v1/chat/completions` | `fast` | The `fast` route |
| `/openai-main/v1/chat/completions` | Upstream model ID | The `openai-main` provider |
| `/acme/v1/chat/completions` | `fast` | The `acme/fast` route |

See [Models and routes](/guides/models/) for naming rules.

## Common errors

| Status | Meaning |
| --- | --- |
| `400` | Invalid request or configuration value |
| `401` | Missing, invalid, disabled, or expired key |
| `403` | Insufficient permissions or OAuth scope |
| `404` | Unknown model, route, or resource |
| `429` | A rate or quota limit applies |
| `5xx` | Gateway or upstream failure; inspect the logs |

After a request, check usage and request records in the console. Body retention depends on logging settings; see [Usage, logs, and audit](/guides/observability/).
