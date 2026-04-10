---
title: API and Routing
description: Full METHOD + PATH routing list and protocol routing rules.
---

## Error format

All errors follow this shape:

```json
{ "error": "..." }
```

## Auth headers and credential sources

Admin and user routes typically use:

- `x-api-key`

Provider routes also support:

- `x-api-key`
- `x-goog-api-key`
- `Authorization: Bearer ...`
- Gemini query `?key=...` (normalized to `x-api-key`)

## Entry and static routes

| Method | Path | Description |
|---|---|---|
| `GET` | `/` | Admin console homepage |
| `GET` | `/assets/{*path}` | Admin static assets |
| `GET` | `/favicon.ico` | Returns `204 No Content` |

## Provider unscoped routes

| Method | Path | Function |
|---|---|---|
| `POST` | `/v1/messages` | Claude-style message generation (unified entrance) |
| `POST` | `/v1/messages/count_tokens` | Claude-style token count |
| `POST` | `/v1/chat/completions` | OpenAI Chat Completions entrance |
| `POST` | `/v1/responses` | OpenAI Responses entrance |
| `GET` | `/v1/responses` | Response upgrade-check entrance (use `POST` in practice) |
| `POST` | `/v1/responses/input_tokens` | OpenAI input token counting |
| `POST` | `/v1/embeddings` | Embedding entrance |
| `POST` | `/v1/responses/compact` | OpenAI Compact entrance |
| `GET` | `/v1/models` | Model list |
| `GET` | `/v1/models/{*model_id}` | Model details |
| `GET` | `/v1beta/models` | Gemini-style model list entrance |
| `GET` | `/v1beta/{*target}` | Gemini-style GET target (`models.get`, etc.) |
| `POST` | `/v1beta/{*target}` | Gemini-style POST target (`generateContent`, `countTokens`, `embedContent`, etc.) |
| `POST` | `/v1/{*target}` | Provider custom `v1` passthrough target |

Rules:

- For unscoped routes, `model` must include provider prefix (for example `openai/gpt-4.1`).
- Gemini path targets must also be provider-resolvable (for example `models/aistudio/gemini-2.5-flash:generateContent`).
- `GET /v1/responses` is upgrade-check logic (upstream WebSocket not implemented yet; use `POST /v1/responses`).

## Provider scoped routes

| Method | Path | Function |
|---|---|---|
| `GET` | `/{provider}/v1/oauth` | Start OAuth authorization flow |
| `GET` | `/{provider}/v1/oauth/callback` | OAuth callback handling |
| `GET` | `/{provider}/v1/usage` | Query upstream usage (supported channels) |
| `GET` | `/{provider}/v1/realtime` | Realtime upgrade entrance |
| `GET` | `/{provider}/v1/realtime/{*tail}` | Realtime upgrade entrance with tail |
| `POST` | `/{provider}/v1/messages` | Claude-style generation |
| `POST` | `/{provider}/v1/messages/count_tokens` | Claude-style token count |
| `POST` | `/{provider}/v1/chat/completions` | OpenAI Chat Completions |
| `POST` | `/{provider}/v1/responses` | OpenAI Responses |
| `GET` | `/{provider}/v1/responses` | Responses upgrade-check entrance |
| `POST` | `/{provider}/v1/responses/input_tokens` | OpenAI input token count |
| `POST` | `/{provider}/v1/embeddings` | Embedding |
| `POST` | `/{provider}/v1/responses/compact` | Compact response entrance |
| `GET` | `/{provider}/v1/models` | Model list |
| `GET` | `/{provider}/v1/models/{*model_id}` | Model details |
| `GET` | `/{provider}/v1beta/models` | Gemini-style model list |
| `GET` | `/{provider}/v1beta/{*target}` | Gemini-style GET target |
| `POST` | `/{provider}/v1beta/{*target}` | Gemini-style POST target |
| `POST` | `/{provider}/v1/{*target}` | Provider `v1` passthrough target |

Built-in channels currently supporting OAuth:

- `codex`
- `claudecode`
- `geminicli`
- `antigravity`

## Supported Gemini methods

| Method | Example path | Function |
|---|---|---|
| `GET` | `/v1beta/models` or `/{provider}/v1beta/models` | `models.list`, list available Gemini models |
| `GET` | `/v1beta/models/{model}` (via `/{*target}`) | `models.get`, query single model details |
| `POST` | `/v1beta/models/{model}:countTokens` | `countTokens`, count input tokens |
| `POST` | `/v1beta/models/{model}:generateContent` | `generateContent`, non-stream generation |
| `POST` | `/v1beta/models/{model}:streamGenerateContent` | `streamGenerateContent`, stream generation (SSE/NDJSON) |
| `POST` | `/v1beta/models/{model}:embedContent` | `embedContent`, vector embedding |

## Admin routes

| Method | Path | Function |
|---|---|---|
| `GET` | `/admin/global-settings` | Read global settings |
| `POST` | `/admin/global-settings/upsert` | Update global settings |
| `POST` | `/admin/system/self_update` | Trigger system self-update |
| `GET` | `/admin/config/export-toml` | Export TOML config |
| `POST` | `/admin/config/import-toml` | Import TOML config |
| `POST` | `/admin/providers/query` | Query providers |
| `POST` | `/admin/providers/upsert` | Create/update provider |
| `POST` | `/admin/providers/delete` | Delete provider |
| `POST` | `/admin/credentials/query` | Query credentials |
| `POST` | `/admin/credentials/upsert` | Create/update credential |
| `POST` | `/admin/credentials/delete` | Delete credential |
| `POST` | `/admin/credential-statuses/query` | Query credential health status |
| `POST` | `/admin/credential-statuses/upsert` | Create/update credential health status |
| `POST` | `/admin/credential-statuses/delete` | Delete credential health status |
| `POST` | `/admin/users/query` | Query users |
| `POST` | `/admin/users/upsert` | Create/update user |
| `POST` | `/admin/users/delete` | Delete user |
| `POST` | `/admin/user-keys/query` | Query user keys |
| `POST` | `/admin/user-keys/upsert` | Create/update user keys |
| `POST` | `/admin/user-keys/delete` | Delete user keys |
| `POST` | `/admin/requests/upstream/query` | Query upstream request audit |
| `POST` | `/admin/requests/downstream/query` | Query downstream request audit |
| `POST` | `/admin/usages/query` | Query usage details |
| `POST` | `/admin/usages/summary` | Query usage summary |

## User routes (`/user/*`)

| Method | Path | Function |
|---|---|---|
| `POST` | `/user/keys/query` | Query current user's keys |
| `POST` | `/user/keys/upsert` | Create/update current user's keys |
| `POST` | `/user/keys/delete` | Delete current user's keys |
| `POST` | `/user/usages/query` | Query current user's usage details |
| `POST` | `/user/usages/summary` | Query current user's usage summary |

User key normalization:

- Stored as `u{user_id}_<raw_key>`
- If input already has this prefix, it stays unchanged

## Request example

```bash
curl -sS http://127.0.0.1:8787/openai/v1/chat/completions \
  -H "x-api-key: <key>" \
  -H "content-type: application/json" \
  -d '{
    "model": "gpt-4.1",
    "messages": [{"role":"user","content":"hello"}],
    "stream": false
  }'
```
