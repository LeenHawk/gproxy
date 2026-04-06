# HTTP API Reference

### Authentication Conventions

- Admin routes accept either a session token from `/login` belonging to a currently enabled admin user, or an API key owned by an admin user.
- User routes under `/user/*` accept a session token from `/login` belonging to any currently enabled user, including admins.
- Provider HTTP routes and Provider WebSocket routes use a user API key.
- Session tokens and API keys may be sent in any of these headers: `Authorization: Bearer <token>`, `x-api-key`, or `x-goog-api-key`.
- Regular HTTP routes accept request bodies up to 50 MiB, while file routes accept bodies up to 500 MiB.

### Login

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| POST | `/login` | None | Logs in any enabled user with username and password and returns a session token plus the current `is_admin` flag. |

### Admin API

#### Health, Reload, and Global Settings

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| GET | `/admin/health` | Admin Session or Admin User API Key | Returns service status, provider count, user count, and a timestamp. |
| POST | `/admin/reload` | Admin Session or Admin User API Key | Reloads all in-memory caches from the database. |
| GET | `/admin/global-settings` | Admin Session or Admin User API Key | Reads the current global configuration. |
| POST | `/admin/global-settings/upsert` | Admin Session or Admin User API Key | Updates the global configuration; if the DSN changes, the process reconnects to the database and bootstraps again. |

#### Providers

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| POST | `/admin/providers/query` | Admin Session or Admin User API Key | Queries providers. |
| POST | `/admin/providers/upsert` | Admin Session or Admin User API Key | Adds or updates one provider. |
| POST | `/admin/providers/delete` | Admin Session or Admin User API Key | Deletes one provider. |
| POST | `/admin/providers/batch-upsert` | Admin Session or Admin User API Key | Adds or updates providers in batch. |
| POST | `/admin/providers/batch-delete` | Admin Session or Admin User API Key | Deletes providers in batch. |

#### Credentials

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| POST | `/admin/credentials/query` | Admin Session or Admin User API Key | Queries provider credentials. |
| POST | `/admin/credentials/upsert` | Admin Session or Admin User API Key | Adds or updates one credential. |
| POST | `/admin/credentials/delete` | Admin Session or Admin User API Key | Deletes one credential. |
| POST | `/admin/credentials/batch-upsert` | Admin Session or Admin User API Key | Adds or updates credentials in batch. |
| POST | `/admin/credentials/batch-delete` | Admin Session or Admin User API Key | Deletes credentials in batch. |
| POST | `/admin/credential-statuses/query` | Admin Session or Admin User API Key | Queries credential health statuses. |
| POST | `/admin/credential-statuses/update` | Admin Session or Admin User API Key | Updates credential health status manually. |

#### Models and Aliases

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| POST | `/admin/models/query` | Admin Session or Admin User API Key | Queries models. |
| POST | `/admin/models/upsert` | Admin Session or Admin User API Key | Adds or updates one model. |
| POST | `/admin/models/delete` | Admin Session or Admin User API Key | Deletes one model. |
| POST | `/admin/models/batch-upsert` | Admin Session or Admin User API Key | Adds or updates models in batch. |
| POST | `/admin/models/batch-delete` | Admin Session or Admin User API Key | Deletes models in batch. |
| POST | `/admin/model-aliases/query` | Admin Session or Admin User API Key | Queries model aliases. |
| POST | `/admin/model-aliases/upsert` | Admin Session or Admin User API Key | Adds or updates one model alias. |
| POST | `/admin/model-aliases/delete` | Admin Session or Admin User API Key | Deletes one model alias. |
| POST | `/admin/model-aliases/batch-upsert` | Admin Session or Admin User API Key | Adds or updates model aliases in batch. |
| POST | `/admin/model-aliases/batch-delete` | Admin Session or Admin User API Key | Deletes model aliases in batch. |

#### Users and Keys

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| POST | `/admin/users/query` | Admin Session or Admin User API Key | Queries users. |
| POST | `/admin/users/upsert` | Admin Session or Admin User API Key | Adds or updates one user. |
| POST | `/admin/users/delete` | Admin Session or Admin User API Key | Deletes one user. |
| POST | `/admin/users/batch-upsert` | Admin Session or Admin User API Key | Adds or updates users in batch. |
| POST | `/admin/users/batch-delete` | Admin Session or Admin User API Key | Deletes users in batch. |
| POST | `/admin/user-keys/query` | Admin Session or Admin User API Key | Queries user API keys. |
| POST | `/admin/user-keys/generate` | Admin Session or Admin User API Key | Generates a new API key for the specified user. |
| POST | `/admin/user-keys/delete` | Admin Session or Admin User API Key | Deletes one user API key. |
| POST | `/admin/user-keys/batch-upsert` | Admin Session or Admin User API Key | Adds or updates user API keys in batch. |
| POST | `/admin/user-keys/batch-delete` | Admin Session or Admin User API Key | Deletes user API keys in batch. |

#### Permissions

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| POST | `/admin/user-permissions/query` | Admin Session or Admin User API Key | Queries user model permissions. |
| POST | `/admin/user-permissions/upsert` | Admin Session or Admin User API Key | Adds or updates one model permission. |
| POST | `/admin/user-permissions/delete` | Admin Session or Admin User API Key | Deletes one model permission. |
| POST | `/admin/user-permissions/batch-upsert` | Admin Session or Admin User API Key | Adds or updates model permissions in batch. |
| POST | `/admin/user-permissions/batch-delete` | Admin Session or Admin User API Key | Deletes model permissions in batch. |

#### File Permissions

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| POST | `/admin/user-file-permissions/query` | Admin Session or Admin User API Key | Queries user file permissions. |
| POST | `/admin/user-file-permissions/upsert` | Admin Session or Admin User API Key | Adds or updates one file permission. |
| POST | `/admin/user-file-permissions/delete` | Admin Session or Admin User API Key | Deletes one file permission. |
| POST | `/admin/user-file-permissions/batch-upsert` | Admin Session or Admin User API Key | Adds or updates file permissions in batch. |
| POST | `/admin/user-file-permissions/batch-delete` | Admin Session or Admin User API Key | Deletes file permissions in batch. |

#### Rate Limits

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| POST | `/admin/user-rate-limits/query` | Admin Session or Admin User API Key | Queries user rate-limit rules. |
| POST | `/admin/user-rate-limits/upsert` | Admin Session or Admin User API Key | Adds or updates one rate-limit rule. |
| POST | `/admin/user-rate-limits/delete` | Admin Session or Admin User API Key | Deletes one rate-limit rule. |
| POST | `/admin/user-rate-limits/batch-upsert` | Admin Session or Admin User API Key | Adds or updates rate-limit rules in batch. |
| POST | `/admin/user-rate-limits/batch-delete` | Admin Session or Admin User API Key | Deletes rate-limit rules in batch. |

#### Requests

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| POST | `/admin/requests/upstream/query` | Admin Session or Admin User API Key | Queries upstream request logs. |
| POST | `/admin/requests/upstream/count` | Admin Session or Admin User API Key | Counts upstream request logs. |
| POST | `/admin/requests/upstream/delete` | Admin Session or Admin User API Key | Deletes one upstream request log or deletes by condition. |
| POST | `/admin/requests/upstream/batch-delete` | Admin Session or Admin User API Key | Deletes upstream request logs in batch. |
| POST | `/admin/requests/downstream/query` | Admin Session or Admin User API Key | Queries downstream request logs. |
| POST | `/admin/requests/downstream/count` | Admin Session or Admin User API Key | Counts downstream request logs. |
| POST | `/admin/requests/downstream/delete` | Admin Session or Admin User API Key | Deletes one downstream request log or deletes by condition. |
| POST | `/admin/requests/downstream/batch-delete` | Admin Session or Admin User API Key | Deletes downstream request logs in batch. |

#### Usages

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| POST | `/admin/usages/query` | Admin Session or Admin User API Key | Queries usage records. |
| POST | `/admin/usages/count` | Admin Session or Admin User API Key | Counts usage records. |
| POST | `/admin/usages/batch-delete` | Admin Session or Admin User API Key | Deletes usage records in batch. |

#### Config

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| POST | `/admin/config/export-toml` | Admin Session or Admin User API Key | Exports the current in-memory and configuration state as TOML. |

#### Update

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| POST | `/admin/update/check` | Admin Session or Admin User API Key | Checks for a new version and returns the download URL. |
| POST | `/admin/update` | Admin Session or Admin User API Key | Downloads, verifies, and replaces the current executable, then schedules a restart. |

### User API

#### Keys

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| POST | `/user/keys/query` | User Session Token | Queries API keys owned by the current user. |
| POST | `/user/keys/generate` | User Session Token | Generates a new API key for the current user. |

#### Quota

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| GET | `/user/quota` | User Session Token | Returns the current user's total quota, used cost, and remaining budget. |

#### Usages

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| POST | `/user/usages/query` | User Session Token | Queries usage records for the current user. |
| POST | `/user/usages/count` | User Session Token | Counts usage records for the current user. |

### Provider HTTP API

#### Scoped Routes

These routes target a provider explicitly through `{provider}` in the path. They all use user authentication and run through request sanitization, model-alias resolution, model extraction, classification, and permission or rate-limit checks.

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| POST | `/{provider}/v1/messages` | User API Key | Claude-style message generation proxy. |
| POST | `/{provider}/v1/messages/count-tokens` | User API Key | Claude-style token counting proxy. |
| POST | `/{provider}/v1/chat/completions` | User API Key | OpenAI Chat Completions proxy. |
| POST | `/{provider}/v1/responses` | User API Key | OpenAI Responses HTTP proxy. |
| POST | `/{provider}/v1/responses/input_tokens` | User API Key | OpenAI Responses input-token counting proxy. |
| POST | `/{provider}/v1/responses/compact` | User API Key | OpenAI Responses compact proxy. |
| POST | `/{provider}/v1/embeddings` | User API Key | Embeddings proxy. |
| POST | `/{provider}/v1/images/generations` | User API Key | Image-generation proxy. |
| POST | `/{provider}/v1/images/edits` | User API Key | Image-editing proxy. |
| GET | `/{provider}/v1/models` | User API Key | Lists models for the specified provider. |
| GET | `/{provider}/v1/models/{*model_id}` | User API Key | Reads details for one model under the specified provider. |
| GET | `/{provider}/v1beta/models` | User API Key | Gemini `v1beta` model-list proxy. |
| POST | `/{provider}/v1beta/{*target}` | User API Key | Gemini `v1beta` arbitrary target-path proxy. |

#### Unscoped Routes

These routes omit the provider in the path and instead resolve the target provider from the model prefix or model alias.

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| POST | `/v1/messages` | User API Key | Claude-style message generation proxy; the provider is resolved from the model prefix or alias. |
| POST | `/v1/messages/count_tokens` | User API Key | Claude-style token counting proxy; the provider is resolved from the model prefix or alias. |
| POST | `/v1/chat/completions` | User API Key | OpenAI Chat Completions proxy; the provider is resolved from the model prefix or alias. |
| POST | `/v1/responses` | User API Key | OpenAI Responses HTTP proxy; the provider is resolved from the model prefix or alias. |
| POST | `/v1/responses/input_tokens` | User API Key | OpenAI Responses input-token counting proxy; the provider is resolved from the model prefix or alias. |
| POST | `/v1/responses/compact` | User API Key | OpenAI Responses compact proxy; the provider is resolved from the model prefix or alias. |
| POST | `/v1/embeddings` | User API Key | Embeddings proxy; the provider is resolved from the model prefix or alias. |
| POST | `/v1/images/generations` | User API Key | Image-generation proxy; the provider is resolved from the model prefix or alias. |
| POST | `/v1/images/edits` | User API Key | Image-editing proxy; the provider is resolved from the model prefix or alias. |
| GET | `/v1/models` | User API Key | Lists models; the provider is resolved from the model prefix or alias. |
| GET | `/v1/models/{*model_id}` | User API Key | Reads details for a single model; the provider is resolved from the model prefix or alias. |
| GET | `/v1beta/models` | User API Key | Gemini `v1beta` model-list proxy; the provider is resolved from the model prefix or alias. |
| POST | `/v1beta/{*target}` | User API Key | Gemini `v1beta` arbitrary target-path proxy; the provider is resolved from the model prefix or alias. |

#### File Routes

File routes come in scoped and unscoped variants. Scoped routes select the provider through `{provider}`, while unscoped routes require the `X-Provider` request header.

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| POST | `/{provider}/v1/files` | User API Key | Uploads a file to the specified provider. |
| GET | `/{provider}/v1/files` | User API Key | Lists files for the specified provider. |
| GET | `/{provider}/v1/files/{file_id}` | User API Key | Reads metadata for the specified file. |
| DELETE | `/{provider}/v1/files/{file_id}` | User API Key | Deletes the specified file. |
| GET | `/{provider}/v1/files/{file_id}/content` | User API Key | Retrieves content for the specified file. |
| POST | `/v1/files` | User API Key | Uploads a file; the target provider is chosen via the `X-Provider` header. |
| GET | `/v1/files` | User API Key | Lists files; the target provider is chosen via the `X-Provider` header. |
| GET | `/v1/files/{file_id}` | User API Key | Reads file metadata; the target provider is chosen via the `X-Provider` header. |
| DELETE | `/v1/files/{file_id}` | User API Key | Deletes a file; the target provider is chosen via the `X-Provider` header. |
| GET | `/v1/files/{file_id}/content` | User API Key | Retrieves file content; the target provider is chosen via the `X-Provider` header. |

### Provider WebSocket API

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| GET | `/{provider}/v1/responses` | User API Key | OpenAI Responses WebSocket; the provider is specified in the path, and `?model=` can be used to choose the initial model. |
| GET | `/{provider}/v1beta/models/{*target_live}` | User API Key | Gemini Live WebSocket; `target_live` looks like `gemini-2.0-flash:streamGenerateContent`. |
| GET | `/v1/responses` | User API Key | Unscoped OpenAI Responses WebSocket; requires `?model=provider/model` or a resolvable model alias. |

### Provider Admin API

These routes do not use the `/admin` prefix, but they still require an admin session token or an API key owned by an admin user.

| Method | Path | Auth | Description |
| --- | --- | --- | --- |
| GET | `/{provider}/v1/oauth` | Admin Session or Admin User API Key | Starts the OAuth flow for the specified provider. |
| GET | `/{provider}/v1/oauth/callback` | Admin Session or Admin User API Key | Handles the OAuth callback for the specified provider. |
| GET | `/{provider}/v1/usage` | Admin Session or Admin User API Key | Queries upstream usage or quota information for the specified provider. |
