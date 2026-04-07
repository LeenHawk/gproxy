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

## Payload Definitions

### Shared JSON Types

Unless noted otherwise, HTTP request and response bodies are JSON.

```ts
type ErrorResponse = {
  error: string;
};

type AckResponse = {
  ok: boolean;
  id?: number;
};

type CountResponse = {
  count: number;
};

type Scope<T> =
  | "All"
  | { Eq: T }
  | { In: T[] };
```

Notes:

- Most query payload fields may be omitted; omission behaves the same as `Scope.All`.
- Timestamp fields backed by `OffsetDateTime` serialize as timestamp strings.
- Byte-array fields such as logged request or response bodies serialize as JSON arrays of numbers.

### Login

```ts
type LoginRequest = {
  username: string;
  password: string;
};

type LoginResponse = {
  user_id: number;
  session_token: string;
  expires_in_secs: number;
  is_admin: boolean;
};
```

### Admin API

#### Health, Reload, and Global Settings

```ts
type HealthResponse = {
  status: "ok";
  provider_count: number;
  user_count: number;
  timestamp_epoch: number;
};

type ReloadResponse = {
  ok: true;
  providers: number;
  users: number;
  keys: number;
  models: number;
  user_files: number;
  claude_files: number;
  aliases: number;
  permissions: number;
  file_permissions: number;
  rate_limits: number;
  quotas: number;
};

type GlobalSettings = {
  host: string;
  port: number;
  proxy?: string | null;
  spoof_emulation: string;
  update_source: string;
  enable_usage: boolean;
  enable_upstream_log: boolean;
  enable_upstream_log_body: boolean;
  enable_downstream_log: boolean;
  enable_downstream_log_body: boolean;
  dsn: string;
  data_dir: string;
};
```

- `GET /admin/health`: no request body, returns `HealthResponse`
- `POST /admin/reload`: no request body, returns `ReloadResponse`
- `GET /admin/global-settings`: no request body, returns `GlobalSettings`
- `POST /admin/global-settings/upsert`: body `GlobalSettings`, returns `AckResponse`

#### Providers

```ts
type ProviderQueryParams = {
  name?: Scope<string>;
  channel?: Scope<string>;
};

type ProviderRow = {
  name: string;
  channel: string;
  settings_json: Record<string, unknown>;
  dispatch_json: Record<string, unknown> | null;
  credential_count: number;
};

type ProviderWrite = {
  id: number;
  name: string;
  channel: string;
  settings_json: string;
  dispatch_json: string;
};

type DeleteProviderPayload = {
  name: string;
};
```

- `POST /admin/providers/query`: body `ProviderQueryParams`, returns `ProviderRow[]`
- `POST /admin/providers/upsert`: body `ProviderWrite`, returns `AckResponse`
- `POST /admin/providers/delete`: body `DeleteProviderPayload`, returns `AckResponse`
- `POST /admin/providers/batch-upsert`: body `ProviderWrite[]`, returns `AckResponse`
- `POST /admin/providers/batch-delete`: body `string[]` of provider names, returns `AckResponse`

#### Credentials

```ts
type CredentialQueryParams = {
  provider_name?: Scope<string>;
};

type CredentialRow = {
  provider: string;
  index: number;
  credential: Record<string, unknown>;
};

type UpsertCredentialPayload = {
  provider_name: string;
  credential: Record<string, unknown>;
};

type DeleteCredentialPayload = {
  provider_name: string;
  index: number;
};

type CredentialHealthQuery = {
  provider_name?: Scope<string>;
};

type CredentialHealthRow = {
  provider: string;
  index: number;
  status: string;
  available: boolean;
};

type UpdateCredentialStatusPayload = {
  provider_name: string;
  index: number;
  status: "healthy" | "dead";
};
```

- `POST /admin/credentials/query`: body `CredentialQueryParams`, returns `CredentialRow[]`
- `POST /admin/credentials/upsert`: body `UpsertCredentialPayload`, returns `AckResponse`
- `POST /admin/credentials/delete`: body `DeleteCredentialPayload`, returns `AckResponse`
- `POST /admin/credentials/batch-upsert`: body `UpsertCredentialPayload[]`, returns `AckResponse`
- `POST /admin/credentials/batch-delete`: body `DeleteCredentialPayload[]`, returns `AckResponse`
- `POST /admin/credential-statuses/query`: body `CredentialHealthQuery`, returns `CredentialHealthRow[]`
- `POST /admin/credential-statuses/update`: body `UpdateCredentialStatusPayload`, returns `AckResponse`

`/admin/credentials/query` returns masked credential JSON from engine memory; secret values are not returned verbatim.

#### Models and Aliases

```ts
type PriceTier = {
  input_tokens_up_to: number;
  price_input_tokens?: number | null;
  price_output_tokens?: number | null;
  price_cache_read_input_tokens?: number | null;
  price_cache_creation_input_tokens?: number | null;
  price_cache_creation_input_tokens_5min?: number | null;
  price_cache_creation_input_tokens_1h?: number | null;
};

type ModelQueryParams = {
  id?: Scope<number>;
  provider_id?: Scope<number>;
  model_id?: Scope<string>;
  enabled?: Scope<boolean>;
  limit?: number;
  offset?: number;
};

type MemoryModelRow = {
  id: number;
  provider_id: number;
  model_id: string;
  display_name?: string | null;
  enabled: boolean;
  price_each_call?: number | null;
  price_tiers: PriceTier[];
};

type ModelWrite = {
  id: number;
  provider_id: number;
  model_id: string;
  display_name?: string | null;
  enabled: boolean;
  price_each_call?: number | null;
  price_tiers_json?: string | null;
};

type DeleteModelPayload = {
  id: number;
};

type MemoryModelAliasRow = {
  alias: string;
  provider_name: string;
  model_id: string;
};

type ModelAliasWrite = {
  id: number;
  alias: string;
  provider_id: number;
  model_id: string;
  enabled: boolean;
};

type DeleteModelAliasPayload = {
  alias: string;
};
```

- `POST /admin/models/query`: body `ModelQueryParams`, returns `MemoryModelRow[]`
- `POST /admin/models/upsert`: body `ModelWrite`, returns `AckResponse`
- `POST /admin/models/delete`: body `DeleteModelPayload`, returns `AckResponse`
- `POST /admin/models/batch-upsert`: body `ModelWrite[]`, returns `AckResponse`
- `POST /admin/models/batch-delete`: body `number[]` of model ids, returns `AckResponse`
- `POST /admin/model-aliases/query`: no request body, returns `MemoryModelAliasRow[]`
- `POST /admin/model-aliases/upsert`: body `ModelAliasWrite`, returns `AckResponse`
- `POST /admin/model-aliases/delete`: body `DeleteModelAliasPayload`, returns `AckResponse`
- `POST /admin/model-aliases/batch-upsert`: body `ModelAliasWrite[]`, returns `AckResponse`
- `POST /admin/model-aliases/batch-delete`: body `DeleteModelAliasPayload[]`, returns `AckResponse`

#### Users and Keys

```ts
type UserQueryParams = {
  id?: Scope<number>;
  name?: Scope<string>;
};

type MemoryUserRow = {
  id: number;
  name: string;
  enabled: boolean;
  is_admin: boolean;
};

type UserWrite = {
  id: number;
  name: string;
  password: string;
  enabled: boolean;
  is_admin: boolean;
};

type DeleteUserPayload = {
  id: number;
};

type UserKeyQueryParams = {
  user_id?: Scope<number>;
};

type MemoryUserKeyRow = {
  id: number;
  user_id: number;
  api_key: string;
  label?: string | null;
  enabled: boolean;
};

type GenerateUserKeyPayload = {
  user_id: number;
  label?: string | null;
};

type GenerateUserKeyResponse = {
  ok: true;
  id: number;
  api_key: string;
};

type DeleteUserKeyPayload = {
  id: number;
};

type BatchGenerateUserKeysPayload = {
  user_id: number;
  count: number;
  label?: string | null;
};

type BatchGenerateUserKeysResponse = {
  ok: true;
  keys: Array<{
    id: number;
    api_key: string;
  }>;
};
```

- `POST /admin/users/query`: body `UserQueryParams`, returns `MemoryUserRow[]`
- `POST /admin/users/upsert`: body `UserWrite`, returns `AckResponse`
- `POST /admin/users/delete`: body `DeleteUserPayload`, returns `AckResponse`
- `POST /admin/users/batch-upsert`: body `UserWrite[]`, returns `AckResponse`
- `POST /admin/users/batch-delete`: body `number[]` of user ids, returns `AckResponse`
- `POST /admin/user-keys/query`: body `UserKeyQueryParams`, returns `MemoryUserKeyRow[]`
- `POST /admin/user-keys/generate`: body `GenerateUserKeyPayload`, returns `GenerateUserKeyResponse`
- `POST /admin/user-keys/delete`: body `DeleteUserKeyPayload`, returns `AckResponse`
- `POST /admin/user-keys/batch-upsert`: body `BatchGenerateUserKeysPayload`, returns `BatchGenerateUserKeysResponse`
- `POST /admin/user-keys/batch-delete`: body `number[]` of user-key ids, returns `AckResponse`

`/admin/user-keys/batch-upsert` is a batch key-generation endpoint despite the route name; it does not accept `UserKeyWrite[]`.

#### Permissions

```ts
type PermissionQueryParams = {
  user_id?: Scope<number>;
  provider_id?: Scope<number>;
  limit?: number;
};

type MemoryPermissionRow = {
  id: number;
  user_id: number;
  provider_id?: number | null;
  model_pattern: string;
};

type UserModelPermissionWrite = {
  id: number;
  user_id: number;
  provider_id?: number | null;
  model_pattern: string;
};

type DeletePermissionPayload = {
  id: number;
};
```

- `POST /admin/user-permissions/query`: body `PermissionQueryParams`, returns `MemoryPermissionRow[]`
- `POST /admin/user-permissions/upsert`: body `UserModelPermissionWrite`, returns `AckResponse`
- `POST /admin/user-permissions/delete`: body `DeletePermissionPayload`, returns `AckResponse`
- `POST /admin/user-permissions/batch-upsert`: body `UserModelPermissionWrite[]`, returns `AckResponse`
- `POST /admin/user-permissions/batch-delete`: body `DeletePermissionPayload[]`, returns `AckResponse`

#### File Permissions

```ts
type FilePermissionQueryParams = {
  user_id?: Scope<number>;
  provider_id?: Scope<number>;
  limit?: number;
};

type MemoryFilePermissionRow = {
  id: number;
  user_id: number;
  provider_id: number;
};

type UserFilePermissionWrite = {
  id: number;
  user_id: number;
  provider_id: number;
};

type DeleteFilePermissionPayload = {
  id: number;
};
```

- `POST /admin/user-file-permissions/query`: body `FilePermissionQueryParams`, returns `MemoryFilePermissionRow[]`
- `POST /admin/user-file-permissions/upsert`: body `UserFilePermissionWrite`, returns `AckResponse`
- `POST /admin/user-file-permissions/delete`: body `DeleteFilePermissionPayload`, returns `AckResponse`
- `POST /admin/user-file-permissions/batch-upsert`: body `UserFilePermissionWrite[]`, returns `AckResponse`
- `POST /admin/user-file-permissions/batch-delete`: body `DeleteFilePermissionPayload[]`, returns `AckResponse`

#### Rate Limits

```ts
type RateLimitQueryParams = {
  user_id?: Scope<number>;
  limit?: number;
};

type MemoryRateLimitRow = {
  user_id: number;
  model_pattern: string;
  rpm?: number | null;
  rpd?: number | null;
  total_tokens?: number | null;
};

type UserRateLimitWrite = {
  id: number;
  user_id: number;
  model_pattern: string;
  rpm?: number | null;
  rpd?: number | null;
  total_tokens?: number | null;
};

type DeleteRateLimitPayload = {
  user_id: number;
  model_pattern: string;
};
```

- `POST /admin/user-rate-limits/query`: body `RateLimitQueryParams`, returns `MemoryRateLimitRow[]`
- `POST /admin/user-rate-limits/upsert`: body `UserRateLimitWrite`, returns `AckResponse`
- `POST /admin/user-rate-limits/delete`: body `DeleteRateLimitPayload`, returns `AckResponse`
- `POST /admin/user-rate-limits/batch-upsert`: body `UserRateLimitWrite[]`, returns `AckResponse`
- `POST /admin/user-rate-limits/batch-delete`: body `DeleteRateLimitPayload[]`, returns `AckResponse`

#### Requests

```ts
type UpstreamRequestQuery = {
  trace_id?: Scope<number>;
  provider_id?: Scope<number>;
  credential_id?: Scope<number>;
  request_url_contains?: string;
  from_unix_ms?: number;
  to_unix_ms?: number;
  cursor_at_unix_ms?: number;
  cursor_trace_id?: number;
  offset?: number;
  limit?: number;
  include_body?: boolean;
};

type UpstreamRequestQueryRow = {
  trace_id: number;
  downstream_trace_id?: number | null;
  at: string;
  internal: boolean;
  provider_id?: number | null;
  credential_id?: number | null;
  request_method: string;
  request_headers_json: Record<string, unknown>;
  request_url?: string | null;
  request_body?: number[] | null;
  response_status?: number | null;
  response_headers_json: Record<string, unknown>;
  response_body?: number[] | null;
  created_at: string;
};

type DownstreamRequestQuery = {
  trace_id?: Scope<number>;
  user_id?: Scope<number>;
  user_key_id?: Scope<number>;
  request_path_contains?: string;
  from_unix_ms?: number;
  to_unix_ms?: number;
  cursor_at_unix_ms?: number;
  cursor_trace_id?: number;
  offset?: number;
  limit?: number;
  include_body?: boolean;
};

type DownstreamRequestQueryRow = {
  trace_id: number;
  at: string;
  internal: boolean;
  user_id?: number | null;
  user_key_id?: number | null;
  request_method: string;
  request_headers_json: Record<string, unknown>;
  request_path: string;
  request_query?: string | null;
  request_body?: number[] | null;
  response_status?: number | null;
  response_headers_json: Record<string, unknown>;
  response_body?: number[] | null;
  created_at: string;
};

type DeleteRequestsPayload = {
  trace_ids: number[];
};
```

- `POST /admin/requests/upstream/query`: body `UpstreamRequestQuery`, returns `UpstreamRequestQueryRow[]`
- `POST /admin/requests/upstream/count`: body `UpstreamRequestQuery`, returns `CountResponse`
- `POST /admin/requests/upstream/delete`: body `DeleteRequestsPayload`, returns `AckResponse`
- `POST /admin/requests/upstream/batch-delete`: body `number[]` of trace ids, returns `AckResponse`
- `POST /admin/requests/downstream/query`: body `DownstreamRequestQuery`, returns `DownstreamRequestQueryRow[]`
- `POST /admin/requests/downstream/count`: body `DownstreamRequestQuery`, returns `CountResponse`
- `POST /admin/requests/downstream/delete`: body `DeleteRequestsPayload`, returns `AckResponse`
- `POST /admin/requests/downstream/batch-delete`: body `number[]` of trace ids, returns `AckResponse`

#### Usages

```ts
type UsageQuery = {
  provider_id?: Scope<number>;
  credential_id?: Scope<number>;
  channel?: Scope<string>;
  model?: Scope<string>;
  user_id?: Scope<number>;
  user_key_id?: Scope<number>;
  from_unix_ms?: number;
  to_unix_ms?: number;
  cursor_at_unix_ms?: number;
  cursor_trace_id?: number;
  offset?: number;
  limit?: number;
};

type UsageQueryRow = {
  trace_id: number;
  downstream_trace_id?: number | null;
  at: string;
  provider_id?: number | null;
  provider_channel?: string | null;
  credential_id?: number | null;
  user_id?: number | null;
  user_key_id?: number | null;
  operation: string;
  protocol: string;
  model?: string | null;
  input_tokens?: number | null;
  output_tokens?: number | null;
  cache_read_input_tokens?: number | null;
  cache_creation_input_tokens?: number | null;
  cache_creation_input_tokens_5min?: number | null;
  cache_creation_input_tokens_1h?: number | null;
};
```

- `POST /admin/usages/query`: body `UsageQuery`, returns `UsageQueryRow[]`
- `POST /admin/usages/count`: body `UsageQuery`, returns `CountResponse`
- `POST /admin/usages/batch-delete`: body `number[]` of usage trace ids, returns `AckResponse`

Passing an empty array to `/admin/usages/batch-delete` deletes all usage rows.

#### Config

`POST /admin/config/export-toml` takes no request body and returns TOML text, not JSON. The logical exported document shape is:

```ts
type GproxyToml = {
  global?: {
    host: string;
    port: number;
    proxy?: string | null;
    spoof_emulation: string;
    update_source: string;
    enable_usage: boolean;
    enable_upstream_log: boolean;
    enable_upstream_log_body: boolean;
    enable_downstream_log: boolean;
    enable_downstream_log_body: boolean;
    dsn: string;
    data_dir: string;
  };
  providers: Array<{
    name: string;
    channel: string;
    settings: Record<string, unknown>;
    credentials: Array<Record<string, unknown>>;
  }>;
  models: Array<{
    provider_name: string;
    model_id: string;
    display_name?: string | null;
    enabled: boolean;
    price_each_call?: number | null;
    price_tiers: PriceTier[];
  }>;
  model_aliases: Array<{
    alias: string;
    provider_name: string;
    model_id: string;
    enabled: boolean;
  }>;
  users: Array<{
    name: string;
    password: string;
    enabled: boolean;
    is_admin: boolean;
    keys: Array<{
      api_key: string;
      label?: string | null;
      enabled: boolean;
    }>;
  }>;
  permissions: Array<{
    user_name: string;
    provider_name?: string | null;
    model_pattern: string;
  }>;
  file_permissions: Array<{
    user_name: string;
    provider_name: string;
  }>;
  rate_limits: Array<{
    user_name: string;
    model_pattern: string;
    rpm?: number | null;
    rpd?: number | null;
    total_tokens?: number | null;
  }>;
  quotas: Array<{
    user_name: string;
    quota: number;
    cost_used: number;
  }>;
};
```

#### Update

```ts
type UpdateCheckResponse = {
  current_version: string;
  latest_version?: string | null;
  update_available: boolean;
  download_url?: string | null;
  update_source: string;
};

type UpdateParams = {
  tag?: string | null;
};

type UpdatePerformResponse = {
  ok: boolean;
  old_version: string;
  new_version: string;
  message: string;
};
```

- `POST /admin/update/check`: no request body, returns `UpdateCheckResponse`
- `POST /admin/update`: body `UpdateParams`, returns `UpdatePerformResponse`

### User API

```ts
type UserKeyRow = {
  api_key: string;
  label?: string | null;
  enabled: boolean;
};

type GenerateKeyPayload = {
  label?: string | null;
};

type GenerateKeyResponse = {
  ok: true;
  api_key: string;
};

type QuotaResponse = {
  user_id: number;
  quota: number;
  cost_used: number;
  remaining: number;
};
```

- `POST /user/keys/query`: no request body, returns `UserKeyRow[]`
- `POST /user/keys/generate`: body `GenerateKeyPayload`, returns `GenerateKeyResponse`
- `GET /user/quota`: no request body, returns `QuotaResponse`
- `POST /user/usages/query`: body `UsageQuery`, returns `UsageQueryRow[]`; any supplied `user_id` is overwritten with the authenticated session user
- `POST /user/usages/count`: body `UsageQuery`, returns `CountResponse`; any supplied `user_id` is overwritten with the authenticated session user

### Provider HTTP API

Provider proxy routes do not introduce gproxy-specific request wrappers. They accept and return protocol-compatible upstream payloads directly, subject to route normalization, provider resolution, alias resolution, permission checks, and optional protocol translation.

Exact Rust type names for the protocol payloads live in `sdk/gproxy-protocol/README.md`. The main route-to-type mapping is:

| Route family | Request type(s) | Response type(s) |
| --- | --- | --- |
| `POST /{provider}/v1/messages`, `POST /v1/messages` | `ClaudeCreateMessageRequest` | `ClaudeCreateMessageResponse`, `ClaudeStreamEvent` |
| `POST /{provider}/v1/messages/count-tokens`, `POST /v1/messages/count_tokens` | `ClaudeCountTokensRequest` | `ClaudeCountTokensResponse` |
| `POST /{provider}/v1/chat/completions`, `POST /v1/chat/completions` | `OpenAiChatCompletionsRequest` | `OpenAiChatCompletionsResponse`, `ChatCompletionChunk` |
| `POST /{provider}/v1/responses`, `POST /v1/responses` | `OpenAiCreateResponseRequest` | `OpenAiCreateResponseResponse`, `ResponseStreamEvent` |
| `POST /{provider}/v1/responses/input_tokens`, `POST /v1/responses/input_tokens` | `OpenAiCountTokensRequest` | `OpenAiCountTokensResponse` |
| `POST /{provider}/v1/responses/compact`, `POST /v1/responses/compact` | `OpenAiCompactRequest` | `OpenAiCompactResponse` |
| `POST /{provider}/v1/embeddings`, `POST /v1/embeddings` | `OpenAiEmbeddingsRequest` | `OpenAiEmbeddingsResponse` |
| `POST /{provider}/v1/images/generations`, `POST /v1/images/generations` | `OpenAiCreateImageRequest` | `OpenAiCreateImageResponse` |
| `POST /{provider}/v1/images/edits`, `POST /v1/images/edits` | `OpenAiCreateImageEditRequest` | `OpenAiCreateImageEditResponse` |
| `GET /{provider}/v1/models`, `GET /v1/models` | protocol-specific model-list request | protocol-specific model-list response |
| `GET /{provider}/v1/models/{*model_id}`, `GET /v1/models/{*model_id}` | protocol-specific model-get request | protocol-specific model-get response |
| `GET /{provider}/v1beta/models`, `GET /v1beta/models` | `GeminiModelListRequest` | `GeminiModelListResponse` |
| `POST /{provider}/v1beta/{*target}`, `POST /v1beta/{*target}` | Gemini-compatible request body | Gemini-compatible response body |

Additional notes:

- Scoped file routes choose the provider from `{provider}`. Unscoped file routes require the `X-Provider` header.
- File uploads use `multipart/form-data`. Current built-in typed coverage in `gproxy-protocol` is Claude-compatible file upload with a required `file` part and an optional `purpose` part.
- `GET /{provider}/v1/files/{file_id}/content` and `GET /v1/files/{file_id}/content` return raw file bytes.

### Provider WebSocket API

Like the HTTP proxy routes, WebSocket routes speak protocol-native message shapes:

| Route family | Client message types | Server message types |
| --- | --- | --- |
| `GET /{provider}/v1/responses`, `GET /v1/responses` | `OpenAiCreateResponseWebSocketClientMessage` | `OpenAiCreateResponseWebSocketServerMessage` |
| `GET /{provider}/v1beta/models/{*target_live}` | `GeminiBidiGenerateContentClientMessage` | `GeminiBidiGenerateContentServerMessage` |

### Provider Admin API

```ts
type OAuthStartResponse = {
  authorize_url: string;
  state: string;
  redirect_uri?: string | null;
  verification_uri?: string | null;
  user_code?: string | null;
  mode?: string | null;
  scope?: string | null;
  instructions?: string | null;
};

type OAuthCallbackResponse = {
  credential: Record<string, unknown>;
  details: Record<string, unknown>;
};
```

- `GET /{provider}/v1/oauth`: query string is forwarded as `Record<string, string>` to the provider channel; returns `OAuthStartResponse`
- `GET /{provider}/v1/oauth/callback`: query string is forwarded as `Record<string, string>`; returns `OAuthCallbackResponse`
- `GET /{provider}/v1/usage`: no stable local schema; returns provider-specific JSON passthrough from the channel implementation
