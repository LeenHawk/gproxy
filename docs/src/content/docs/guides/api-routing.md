---
title: API Routing Reference
description: Complete route reference for GPROXY v1 — every method, path, auth requirement, and protocol.
---

## Error format

All errors return JSON:

```json
{"error": "human-readable message"}
```

HTTP status codes follow standard semantics (400, 401, 403, 404, 429, 500, 503).

## Auth header extraction

GPROXY checks credentials in this order and uses the first non-empty value:

1. `Authorization: Bearer <token>` header
2. `x-api-key` header
3. `x-goog-api-key` header
4. `?key=<value>` query parameter (Gemini-native clients)

Session tokens (from `/login`) start with `sess-` and are only valid for `/admin/*` and `/user/*` routes.

API keys are valid for provider proxy routes and admin routes (if owned by an admin user).

## Entry routes

| Method | Path | Description |
|---|---|---|
| `GET` | `/` | Redirect (308) to `/console/login` |
| `GET` | `/console` | Console SPA (serves `index.html`) |
| `GET` | `/console/{*path}` | Console SPA (all sub-routes, static assets) |
| `POST` | `/login` | Password login, returns session token |

### POST `/login`

Request:

```json
{"username": "admin", "password": "..."}
```

Response:

```json
{
  "user_id": 1,
  "session_token": "sess-...",
  "expires_in_secs": 86400,
  "is_admin": true
}
```

## Admin routes (`/admin/*`)

Require admin auth: either a session token from an admin user, or an API key owned by an admin user. Non-admin keys get `403 forbidden`.

### System

| Method | Path | Description |
|---|---|---|
| `GET` | `/admin/health` | Health check |
| `POST` | `/admin/reload` | Reload all in-memory caches from database |

### Global settings

| Method | Path | Description |
|---|---|---|
| `GET` | `/admin/global-settings` | Read global settings |
| `POST` | `/admin/global-settings/upsert` | Update global settings |

### Providers

| Method | Path | Description |
|---|---|---|
| `POST` | `/admin/providers/query` | Query providers |
| `POST` | `/admin/providers/default-dispatch` | Get default dispatch table for a channel type |
| `POST` | `/admin/providers/upsert` | Create or update a provider |
| `POST` | `/admin/providers/delete` | Delete a provider |
| `POST` | `/admin/providers/batch-upsert` | Batch create/update providers |
| `POST` | `/admin/providers/batch-delete` | Batch delete providers |

### Credentials

| Method | Path | Description |
|---|---|---|
| `POST` | `/admin/credentials/query` | Query credentials |
| `POST` | `/admin/credentials/upsert` | Create or update a credential |
| `POST` | `/admin/credentials/delete` | Delete a credential |
| `POST` | `/admin/credentials/batch-upsert` | Batch create/update credentials |
| `POST` | `/admin/credentials/batch-delete` | Batch delete credentials |

### Credential statuses

| Method | Path | Description |
|---|---|---|
| `POST` | `/admin/credential-statuses/query` | Query credential health statuses |
| `POST` | `/admin/credential-statuses/update` | Update a credential's health status |

### Models

| Method | Path | Description |
|---|---|---|
| `POST` | `/admin/models/query` | Query models |
| `POST` | `/admin/models/upsert` | Create or update a model |
| `POST` | `/admin/models/delete` | Delete a model |
| `POST` | `/admin/models/batch-upsert` | Batch create/update models |
| `POST` | `/admin/models/batch-delete` | Batch delete models |

### Model aliases

| Method | Path | Description |
|---|---|---|
| `POST` | `/admin/model-aliases/query` | Query model aliases |
| `POST` | `/admin/model-aliases/upsert` | Create or update a model alias |
| `POST` | `/admin/model-aliases/delete` | Delete a model alias |
| `POST` | `/admin/model-aliases/batch-upsert` | Batch create/update model aliases |
| `POST` | `/admin/model-aliases/batch-delete` | Batch delete model aliases |

### Users

| Method | Path | Description |
|---|---|---|
| `POST` | `/admin/users/query` | Query users |
| `POST` | `/admin/users/upsert` | Create or update a user |
| `POST` | `/admin/users/delete` | Delete a user |
| `POST` | `/admin/users/batch-upsert` | Batch create/update users |
| `POST` | `/admin/users/batch-delete` | Batch delete users |

### User keys

| Method | Path | Description |
|---|---|---|
| `POST` | `/admin/user-keys/query` | Query user API keys |
| `POST` | `/admin/user-keys/generate` | Generate a new API key for a user |
| `POST` | `/admin/user-keys/update-enabled` | Enable or disable a user key |
| `POST` | `/admin/user-keys/delete` | Delete a user key |
| `POST` | `/admin/user-keys/batch-upsert` | Batch create/update user keys |
| `POST` | `/admin/user-keys/batch-delete` | Batch delete user keys |

### User quotas

| Method | Path | Description |
|---|---|---|
| `POST` | `/admin/user-quotas/query` | Query user quotas |
| `POST` | `/admin/user-quotas/upsert` | Create or update a user quota |

### User permissions

| Method | Path | Description |
|---|---|---|
| `POST` | `/admin/user-permissions/query` | Query user permissions |
| `POST` | `/admin/user-permissions/upsert` | Create or update a user permission |
| `POST` | `/admin/user-permissions/delete` | Delete a user permission |
| `POST` | `/admin/user-permissions/batch-upsert` | Batch create/update user permissions |
| `POST` | `/admin/user-permissions/batch-delete` | Batch delete user permissions |

### User file permissions

| Method | Path | Description |
|---|---|---|
| `POST` | `/admin/user-file-permissions/query` | Query user file permissions |
| `POST` | `/admin/user-file-permissions/upsert` | Create or update a user file permission |
| `POST` | `/admin/user-file-permissions/delete` | Delete a user file permission |
| `POST` | `/admin/user-file-permissions/batch-upsert` | Batch create/update user file permissions |
| `POST` | `/admin/user-file-permissions/batch-delete` | Batch delete user file permissions |

### User rate limits

| Method | Path | Description |
|---|---|---|
| `POST` | `/admin/user-rate-limits/query` | Query user rate limits |
| `POST` | `/admin/user-rate-limits/upsert` | Create or update a user rate limit |
| `POST` | `/admin/user-rate-limits/delete` | Delete a user rate limit |
| `POST` | `/admin/user-rate-limits/batch-upsert` | Batch create/update user rate limits |
| `POST` | `/admin/user-rate-limits/batch-delete` | Batch delete user rate limits |

### Requests (audit log)

| Method | Path | Description |
|---|---|---|
| `POST` | `/admin/requests/upstream/query` | Query upstream request logs |
| `POST` | `/admin/requests/upstream/count` | Count upstream request logs |
| `POST` | `/admin/requests/upstream/clear` | Clear upstream request payloads (keep metadata) |
| `POST` | `/admin/requests/upstream/delete` | Delete upstream request logs |
| `POST` | `/admin/requests/upstream/batch-delete` | Batch delete upstream request logs |
| `POST` | `/admin/requests/downstream/query` | Query downstream request logs |
| `POST` | `/admin/requests/downstream/count` | Count downstream request logs |
| `POST` | `/admin/requests/downstream/clear` | Clear downstream request payloads (keep metadata) |
| `POST` | `/admin/requests/downstream/delete` | Delete downstream request logs |
| `POST` | `/admin/requests/downstream/batch-delete` | Batch delete downstream request logs |

### Usages

| Method | Path | Description |
|---|---|---|
| `POST` | `/admin/usages/query` | Query usage records |
| `POST` | `/admin/usages/count` | Count usage records |
| `POST` | `/admin/usages/summary` | Aggregated usage summary |
| `POST` | `/admin/usages/batch-delete` | Batch delete usage records |

### Config export

| Method | Path | Description |
|---|---|---|
| `POST` | `/admin/config/export-toml` | Export current configuration as TOML |

### Self-update

| Method | Path | Description |
|---|---|---|
| `POST` | `/admin/update/check` | Check for available updates |
| `POST` | `/admin/update` | Perform self-update |

## User routes (`/user/*`)

Require a session token (from `/login`). Any authenticated user, not just admins. API keys are not accepted on these routes -- this prevents a leaked inference key from being used to generate new keys or enumerate existing ones.

| Method | Path | Description |
|---|---|---|
| `POST` | `/user/keys/query` | Query current user's API keys |
| `POST` | `/user/keys/generate` | Generate a new API key |
| `POST` | `/user/keys/update-enabled` | Enable or disable own key |
| `POST` | `/user/keys/delete` | Delete own key |
| `GET` | `/user/quota` | Get current user's quota |
| `POST` | `/user/usages/query` | Query current user's usage records |
| `POST` | `/user/usages/count` | Count current user's usage records |
| `POST` | `/user/usages/summary` | Current user's aggregated usage summary |

## Provider scoped routes (`/{provider}/v1/*`)

Require API key auth. The `{provider}` path segment determines which provider to use. The model field in the request body does not need a provider prefix.

### Inference

| Method | Path | Description |
|---|---|---|
| `POST` | `/{provider}/v1/messages` | Claude Messages API |
| `POST` | `/{provider}/v1/messages/count-tokens` | Claude token counting |
| `POST` | `/{provider}/v1/chat/completions` | OpenAI Chat Completions |
| `POST` | `/{provider}/v1/responses` | OpenAI Responses API |
| `POST` | `/{provider}/v1/responses/input_tokens` | OpenAI Responses input token counting |
| `POST` | `/{provider}/v1/responses/compact` | OpenAI Responses compact mode |
| `POST` | `/{provider}/v1/embeddings` | Embeddings |
| `POST` | `/{provider}/v1/images/generations` | Image generation |
| `POST` | `/{provider}/v1/images/edits` | Image editing |

### Models

| Method | Path | Description |
|---|---|---|
| `GET` | `/{provider}/v1/models` | List models |
| `GET` | `/{provider}/v1/models/{*model_id}` | Get model details |

### Gemini native

| Method | Path | Description |
|---|---|---|
| `GET` | `/{provider}/v1beta/models` | Gemini model list |
| `POST` | `/{provider}/v1beta/models/{*target}` | Gemini generateContent, streamGenerateContent, countTokens, embedContent |
| `POST` | `/{provider}/v1beta/{*target}` | Gemini v1beta catch-all |

### Files

| Method | Path | Description |
|---|---|---|
| `POST` | `/{provider}/v1/files` | Upload file |
| `GET` | `/{provider}/v1/files` | List files |
| `GET` | `/{provider}/v1/files/{file_id}` | Get file metadata |
| `DELETE` | `/{provider}/v1/files/{file_id}` | Delete file |
| `GET` | `/{provider}/v1/files/{file_id}/content` | Download file content |

### OAuth and usage (admin only)

| Method | Path | Description |
|---|---|---|
| `GET` | `/{provider}/v1/oauth` | Start OAuth authorization flow |
| `GET` | `/{provider}/v1/oauth/callback` | OAuth callback |
| `GET` | `/{provider}/v1/usage` | Query upstream provider usage/quota |

## Provider unscoped routes (`/v1/*`, `/v1beta/*`)

Same endpoints as scoped routes, but without the `{provider}` path prefix. The provider is resolved from the model field in the request body, which must include a provider prefix (e.g. `openai/gpt-4.1`), or match a configured model alias.

### Inference

| Method | Path | Description |
|---|---|---|
| `POST` | `/v1/messages` | Claude Messages (provider from model prefix) |
| `POST` | `/v1/messages/count_tokens` | Claude token counting |
| `POST` | `/v1/chat/completions` | OpenAI Chat Completions |
| `POST` | `/v1/responses` | OpenAI Responses |
| `POST` | `/v1/responses/input_tokens` | OpenAI Responses input token counting |
| `POST` | `/v1/responses/compact` | OpenAI Responses compact mode |
| `POST` | `/v1/embeddings` | Embeddings |
| `POST` | `/v1/images/generations` | Image generation |
| `POST` | `/v1/images/edits` | Image editing |

### Models

| Method | Path | Description |
|---|---|---|
| `GET` | `/v1/models` | List models (all providers) |
| `GET` | `/v1/models/{*model_id}` | Get model details |

### Gemini native

| Method | Path | Description |
|---|---|---|
| `GET` | `/v1beta/models` | Gemini model list |
| `POST` | `/v1beta/{*target}` | Gemini v1beta catch-all |

### Files (unscoped)

Unscoped file routes resolve the provider from the `X-Provider` header rather than the model field.

| Method | Path | Description |
|---|---|---|
| `POST` | `/v1/files` | Upload file |
| `GET` | `/v1/files` | List files |
| `GET` | `/v1/files/{file_id}` | Get file metadata |
| `DELETE` | `/v1/files/{file_id}` | Delete file |
| `GET` | `/v1/files/{file_id}/content` | Download file content |

## WebSocket routes

| Method | Path | Description |
|---|---|---|
| `GET` | `/{provider}/v1/responses` | OpenAI Responses streaming via WebSocket |
| `GET` | `/{provider}/v1beta/models/{*target}` | Gemini Live API |
| `GET` | `/v1/responses` | OpenAI Responses WebSocket (unscoped, provider from model prefix) |

WebSocket connections use the same auth as HTTP provider routes (API key in header or query parameter).

## Request examples

### Scoped request (provider in URL)

```bash
curl -sS http://127.0.0.1:8787/openai/v1/chat/completions \
  -H "Authorization: Bearer <your-api-key>" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4.1",
    "messages": [{"role": "user", "content": "hello"}],
    "stream": false
  }'
```

### Unscoped request (provider in model field)

```bash
curl -sS http://127.0.0.1:8787/v1/chat/completions \
  -H "x-api-key: <your-api-key>" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "openai/gpt-4.1",
    "messages": [{"role": "user", "content": "hello"}],
    "stream": false
  }'
```

The provider prefix (`openai/`) is stripped before the request is sent upstream.

### Gemini native (query parameter auth)

```bash
curl -sS "http://127.0.0.1:8787/aistudio/v1beta/models/gemini-2.5-flash:generateContent?key=<your-api-key>" \
  -H "Content-Type: application/json" \
  -d '{
    "contents": [{"parts": [{"text": "hello"}]}]
  }'
```
