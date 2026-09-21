---
title: First Request
description: Send OpenAI, Claude and Gemini requests through GPROXY, stream them, use the three mounts, and read the errors it answers with.
---

GPROXY answers on the native paths of each accepted wire format. A gateway API
key authenticates the caller; the `model` name selects where the request goes.
The examples assume an exposed model named `fast` and a key created as in the
[Quick Start](/getting-started/quick-start/).

```sh
export GPROXY_KEY='sk-…'
```

## Authentication

Send the key in any of these headers, on any path:

```text
Authorization: Bearer sk-…
x-api-key: sk-…
x-goog-api-key: sk-…
```

All three reach the same identity. A key is also looked up under the digest of
its payload with a leading `sk-` or `at-` removed, so the same secret spelled
`sk-X`, `at-X` or bare `X` is **one** key: quota and usage follow the key
rather than how it was typed.

Every authentication failure is `401`, never `403` — a disabled key, an expired
key, a revoked grant and a key that never existed are indistinguishable from
outside:

```json
{"error":{"code":"unauthorized","message":"unauthorized"}}
```

## OpenAI Chat Completions

```sh
curl -s http://127.0.0.1:7070/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"fast","messages":[{"role":"user","content":"Say hello."}]}'
```

## OpenAI Responses

```sh
curl -s http://127.0.0.1:7070/v1/responses \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"fast","input":"Say hello."}'
```

## Claude Messages

```sh
curl -s http://127.0.0.1:7070/v1/messages \
  -H "x-api-key: $GPROXY_KEY" \
  -H 'anthropic-version: 2023-06-01' \
  -H 'content-type: application/json' \
  -d '{"model":"fast","max_tokens":256,
       "messages":[{"role":"user","content":"Say hello."}]}'
```

When the member behind `fast` is an OpenAI-shaped upstream, the answer still
arrives as Claude Messages:

```json
{"type":"message","id":"chatcmpl-mock-1",
 "content":[{"type":"text","text":"Hello from the mock upstream."}],
 "model":"gpt-4o-mini","role":"assistant","stop_reason":"end_turn",
 "usage":{"input_tokens":11,"output_tokens":7}}
```

## Gemini GenerateContent

Gemini carries the model in the path:

```sh
curl -s "http://127.0.0.1:7070/v1beta/models/fast:generateContent" \
  -H "x-goog-api-key: $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"contents":[{"parts":[{"text":"Say hello."}]}]}'
```

```json
{"candidates":[{"content":{"parts":[{"text":"Hello from the mock upstream."}],
 "role":"model"},"finishReason":"STOP","index":0}],
 "usageMetadata":{"promptTokenCount":11,"totalTokenCount":18},
 "modelVersion":"gpt-4o-mini","responseId":"chatcmpl-mock-1"}
```

Three paths (`/v1/models`, `/v1/models/{id}`, `/v1/files`, …) are spelled
identically by OpenAI, Claude and Gemini. The tiebreak is the client's own
authentication header — `x-goog-api-key` is Gemini's, `anthropic-version` is
Claude's — because that is evidence the client supplied about itself. With no
evidence the OpenAI row wins.

## Streaming

For the three body-flag dialects, add `"stream": true`. The response is
server-sent events in that format's own event shape:

```sh
curl -sN http://127.0.0.1:7070/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"fast","stream":true,
       "messages":[{"role":"user","content":"Count to three."}]}'
```

`/v1/messages` with `"stream": true` over an OpenAI upstream is translated
event by event, not buffered and re-emitted:

```text
event: message_start
data: {"type":"message_start","message":{"type":"message","id":"chatcmpl-mock-1",…}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}
```

A path with `stream: true` is a **different operation** from the same path
without it — separate routing rules, separate settlement — which is why the
flag is read at ingress rather than left for a channel to notice.

Gemini says it in the path instead. Without a query the stream is Gemini's
incremental JSON array; `?alt=sse` selects server-sent events:

```sh
curl -sN "http://127.0.0.1:7070/v1beta/models/fast:streamGenerateContent?alt=sse" \
  -H "x-goog-api-key: $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"contents":[{"parts":[{"text":"Count to three."}]}]}'
```

The caller receives the framing it asked for regardless of what the upstream
produced.

## Listing Models

```sh
curl -s http://127.0.0.1:7070/v1/models -H "Authorization: Bearer $GPROXY_KEY"
```

`GET /v1/models` is **forwarded to a provider** in v4 and answers with that
upstream's own catalogue; it is not synthesized from your configuration. The
list of names *you* publish is the portal's, and it marks each one with whether
this caller may call it:

```sh
curl -s http://127.0.0.1:7070/portal/api/models -H "Authorization: Bearer $GPROXY_KEY"
```

```json
[{"name":"custom/gpt-4o-mini","providerCount":1,"channelIds":["custom"],"permitted":true},
 {"name":"fast","providerCount":1,"channelIds":["custom"],"permitted":true}]
```

Nothing is omitted from that list. A name the caller's rules do not reach stays
in it with `permitted: false`, because a list that silently omits makes "this
model 404s" and "you are not allowed this model" the same observation.

## The Three Mounts

```sh
# aggregated — the model name decides everything
curl -s http://127.0.0.1:7070/v1/chat/completions … -d '{"model":"fast",…}'

# namespace — exposing `acme/fast` creates the namespace `acme`
curl -s http://127.0.0.1:7070/acme/v1/chat/completions … -d '{"model":"fast",…}'

# provider — that provider and nothing else
curl -s http://127.0.0.1:7070/openai-main/v1/chat/completions … -d '{"model":"gpt-4o-mini",…}'
```

A mount narrows by **prefixing the model name**: `/acme/v1/messages` with
`{"model":"fast"}` resolves `acme/fast`, which is the resolver's own
`prefix/model` grammar rather than a second rule. A name that already carries
the prefix is left alone, so a client may spell it either way.

A prefix is only stripped when what remains is itself a surface this gateway
serves. That rule is what stops a provider named `backend-api` from eating
`/backend-api/codex/responses`, which is a real Codex path.

## Errors

The product surfaces answer

```json
{"error":{"code":"unknown_model","message":"unknown model `nope`"}}
```

| Status | Means |
| --- | --- |
| `400` | the request is malformed, or names something invalid |
| `401` | the credential is missing, unknown, disabled, expired or revoked |
| `403` | admission refused it: a permission, or the OAuth operation baseline |
| `404` | the model name, the route or the row does not exist |
| `429` | a rate limit; a cache that cannot answer also refuses here |
| `5xx` | the instance or every upstream attempt failed |

A 5xx message is never returned to the caller — it can quote a DSN, a row or a
header. The `code` is; the text goes to the operator's log.

The OAuth endpoints answer RFC 6749 §5.2's envelope instead, because the RFC
says `error` is a string and a client that finds an object there cannot tell
"re-run the login" from "keep polling":

```json
{"error":"invalid_grant","error_description":"…"}
```

## Finding the Request Afterwards

There is no `x-request-id` response header in v4. A request's own id is
recorded with its usage row and its capture, and the caller's own view of both
is the portal:

```sh
curl -s http://127.0.0.1:7070/portal/api/usage    -H "Authorization: Bearer $GPROXY_KEY"
curl -s http://127.0.0.1:7070/portal/api/requests -H "Authorization: Bearer $GPROXY_KEY"
```

See [Usage, Logs & Audit](/guides/observability/) for what is recorded, what is
redacted, and what the HTTP host does not yet expose.
