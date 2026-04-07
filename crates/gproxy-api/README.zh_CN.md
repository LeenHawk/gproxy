# HTTP API 参考

### 鉴权约定

- 管理员路由接受 `/login` 返回且当前用户仍为管理员的 Session Token，或管理员用户自己的 API Key。
- `/user/*` 路由接受 `/login` 返回的 Session Token，任何当前启用的用户都可以访问，包括管理员。
- Provider HTTP 路由和 Provider WebSocket 路由使用用户 API Key。
- Session Token 和 API Key 都可以放在以下任一请求头中：`Authorization: Bearer <token>`、`x-api-key`、`x-goog-api-key`。
- 普通 HTTP 路由请求体上限为 50 MiB；文件路由请求体上限为 500 MiB。

### Login

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| POST | `/login` | 无 | 任意启用用户使用用户名 + 密码登录，返回统一 Session Token 和当前 `is_admin` 标记。 |

### Admin API

#### 健康、重载与全局设置

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| GET | `/admin/health` | 管理员 Session 或管理员用户 API Key | 返回服务状态、Provider 数量、用户数量和时间戳。 |
| POST | `/admin/reload` | 管理员 Session 或管理员用户 API Key | 从数据库重新装载所有内存缓存。 |
| GET | `/admin/global-settings` | 管理员 Session 或管理员用户 API Key | 读取当前全局配置。 |
| POST | `/admin/global-settings/upsert` | 管理员 Session 或管理员用户 API Key | 更新全局配置；如果 DSN 改变，会重连数据库并重新 bootstrap。 |

#### Providers

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| POST | `/admin/providers/query` | 管理员 Session 或管理员用户 API Key | 查询 Provider 列表。 |
| POST | `/admin/providers/upsert` | 管理员 Session 或管理员用户 API Key | 新增或更新单个 Provider。 |
| POST | `/admin/providers/delete` | 管理员 Session 或管理员用户 API Key | 删除单个 Provider。 |
| POST | `/admin/providers/batch-upsert` | 管理员 Session 或管理员用户 API Key | 批量新增或更新 Provider。 |
| POST | `/admin/providers/batch-delete` | 管理员 Session 或管理员用户 API Key | 批量删除 Provider。 |

#### Credentials

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| POST | `/admin/credentials/query` | 管理员 Session 或管理员用户 API Key | 查询 Provider 凭证。 |
| POST | `/admin/credentials/upsert` | 管理员 Session 或管理员用户 API Key | 新增或更新单个凭证。 |
| POST | `/admin/credentials/delete` | 管理员 Session 或管理员用户 API Key | 删除单个凭证。 |
| POST | `/admin/credentials/batch-upsert` | 管理员 Session 或管理员用户 API Key | 批量新增或更新凭证。 |
| POST | `/admin/credentials/batch-delete` | 管理员 Session 或管理员用户 API Key | 批量删除凭证。 |
| POST | `/admin/credential-statuses/query` | 管理员 Session 或管理员用户 API Key | 查询凭证健康状态。 |
| POST | `/admin/credential-statuses/update` | 管理员 Session 或管理员用户 API Key | 手动更新凭证健康状态。 |

#### Models 与 Aliases

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| POST | `/admin/models/query` | 管理员 Session 或管理员用户 API Key | 查询模型列表。 |
| POST | `/admin/models/upsert` | 管理员 Session 或管理员用户 API Key | 新增或更新单个模型。 |
| POST | `/admin/models/delete` | 管理员 Session 或管理员用户 API Key | 删除单个模型。 |
| POST | `/admin/models/batch-upsert` | 管理员 Session 或管理员用户 API Key | 批量新增或更新模型。 |
| POST | `/admin/models/batch-delete` | 管理员 Session 或管理员用户 API Key | 批量删除模型。 |
| POST | `/admin/model-aliases/query` | 管理员 Session 或管理员用户 API Key | 查询模型别名。 |
| POST | `/admin/model-aliases/upsert` | 管理员 Session 或管理员用户 API Key | 新增或更新单个模型别名。 |
| POST | `/admin/model-aliases/delete` | 管理员 Session 或管理员用户 API Key | 删除单个模型别名。 |
| POST | `/admin/model-aliases/batch-upsert` | 管理员 Session 或管理员用户 API Key | 批量新增或更新模型别名。 |
| POST | `/admin/model-aliases/batch-delete` | 管理员 Session 或管理员用户 API Key | 批量删除模型别名。 |

#### Users 与 Keys

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| POST | `/admin/users/query` | 管理员 Session 或管理员用户 API Key | 查询用户列表。 |
| POST | `/admin/users/upsert` | 管理员 Session 或管理员用户 API Key | 新增或更新单个用户。 |
| POST | `/admin/users/delete` | 管理员 Session 或管理员用户 API Key | 删除单个用户。 |
| POST | `/admin/users/batch-upsert` | 管理员 Session 或管理员用户 API Key | 批量新增或更新用户。 |
| POST | `/admin/users/batch-delete` | 管理员 Session 或管理员用户 API Key | 批量删除用户。 |
| POST | `/admin/user-keys/query` | 管理员 Session 或管理员用户 API Key | 查询用户 API Key。 |
| POST | `/admin/user-keys/generate` | 管理员 Session 或管理员用户 API Key | 为指定用户生成新的 API Key。 |
| POST | `/admin/user-keys/delete` | 管理员 Session 或管理员用户 API Key | 删除单个用户 API Key。 |
| POST | `/admin/user-keys/batch-upsert` | 管理员 Session 或管理员用户 API Key | 批量新增或更新用户 API Key。 |
| POST | `/admin/user-keys/batch-delete` | 管理员 Session 或管理员用户 API Key | 批量删除用户 API Key。 |

#### Permissions

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| POST | `/admin/user-permissions/query` | 管理员 Session 或管理员用户 API Key | 查询用户模型权限。 |
| POST | `/admin/user-permissions/upsert` | 管理员 Session 或管理员用户 API Key | 新增或更新单条模型权限。 |
| POST | `/admin/user-permissions/delete` | 管理员 Session 或管理员用户 API Key | 删除单条模型权限。 |
| POST | `/admin/user-permissions/batch-upsert` | 管理员 Session 或管理员用户 API Key | 批量新增或更新模型权限。 |
| POST | `/admin/user-permissions/batch-delete` | 管理员 Session 或管理员用户 API Key | 批量删除模型权限。 |

#### File Permissions

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| POST | `/admin/user-file-permissions/query` | 管理员 Session 或管理员用户 API Key | 查询用户文件权限。 |
| POST | `/admin/user-file-permissions/upsert` | 管理员 Session 或管理员用户 API Key | 新增或更新单条文件权限。 |
| POST | `/admin/user-file-permissions/delete` | 管理员 Session 或管理员用户 API Key | 删除单条文件权限。 |
| POST | `/admin/user-file-permissions/batch-upsert` | 管理员 Session 或管理员用户 API Key | 批量新增或更新文件权限。 |
| POST | `/admin/user-file-permissions/batch-delete` | 管理员 Session 或管理员用户 API Key | 批量删除文件权限。 |

#### Rate Limits

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| POST | `/admin/user-rate-limits/query` | 管理员 Session 或管理员用户 API Key | 查询用户限流规则。 |
| POST | `/admin/user-rate-limits/upsert` | 管理员 Session 或管理员用户 API Key | 新增或更新单条限流规则。 |
| POST | `/admin/user-rate-limits/delete` | 管理员 Session 或管理员用户 API Key | 删除单条限流规则。 |
| POST | `/admin/user-rate-limits/batch-upsert` | 管理员 Session 或管理员用户 API Key | 批量新增或更新限流规则。 |
| POST | `/admin/user-rate-limits/batch-delete` | 管理员 Session 或管理员用户 API Key | 批量删除限流规则。 |

#### Requests

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| POST | `/admin/requests/upstream/query` | 管理员 Session 或管理员用户 API Key | 查询上游请求日志。 |
| POST | `/admin/requests/upstream/count` | 管理员 Session 或管理员用户 API Key | 统计上游请求日志。 |
| POST | `/admin/requests/upstream/delete` | 管理员 Session 或管理员用户 API Key | 删除单条或按条件删除上游请求日志。 |
| POST | `/admin/requests/upstream/batch-delete` | 管理员 Session 或管理员用户 API Key | 批量删除上游请求日志。 |
| POST | `/admin/requests/downstream/query` | 管理员 Session 或管理员用户 API Key | 查询下游请求日志。 |
| POST | `/admin/requests/downstream/count` | 管理员 Session 或管理员用户 API Key | 统计下游请求日志。 |
| POST | `/admin/requests/downstream/delete` | 管理员 Session 或管理员用户 API Key | 删除单条或按条件删除下游请求日志。 |
| POST | `/admin/requests/downstream/batch-delete` | 管理员 Session 或管理员用户 API Key | 批量删除下游请求日志。 |

#### Usages

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| POST | `/admin/usages/query` | 管理员 Session 或管理员用户 API Key | 查询 usage 记录。 |
| POST | `/admin/usages/count` | 管理员 Session 或管理员用户 API Key | 统计 usage 记录。 |
| POST | `/admin/usages/batch-delete` | 管理员 Session 或管理员用户 API Key | 批量删除 usage 记录。 |

#### Config

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| POST | `/admin/config/export-toml` | 管理员 Session 或管理员用户 API Key | 导出当前内存 / 配置状态为 TOML。 |

#### Update

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| POST | `/admin/update/check` | 管理员 Session 或管理员用户 API Key | 检查是否有新版本，并返回下载地址。 |
| POST | `/admin/update` | 管理员 Session 或管理员用户 API Key | 下载、校验并替换当前可执行文件，然后调度重启。 |

### User API

#### Keys

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| POST | `/user/keys/query` | 用户 Session Token | 查询当前用户自己的 API Key。 |
| POST | `/user/keys/generate` | 用户 Session Token | 为当前用户生成新的 API Key。 |

#### Quota

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| GET | `/user/quota` | 用户 Session Token | 返回当前用户的总配额、已用成本和剩余预算。 |

#### Usages

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| POST | `/user/usages/query` | 用户 Session Token | 查询当前用户的 usage 记录。 |
| POST | `/user/usages/count` | 用户 Session Token | 统计当前用户的 usage 记录。 |

### Provider HTTP API

#### Scoped 路由

这些路由通过路径中的 `{provider}` 指定目标 Provider，全部走用户鉴权，并经过请求净化、模型别名解析、模型提取、分类和权限 / 限流检查。

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| POST | `/{provider}/v1/messages` | 用户 API Key | Claude 风格消息生成代理。 |
| POST | `/{provider}/v1/messages/count-tokens` | 用户 API Key | Claude 风格 token 统计代理。 |
| POST | `/{provider}/v1/chat/completions` | 用户 API Key | OpenAI Chat Completions 代理。 |
| POST | `/{provider}/v1/responses` | 用户 API Key | OpenAI Responses HTTP 代理。 |
| POST | `/{provider}/v1/responses/input_tokens` | 用户 API Key | OpenAI Responses input token 统计代理。 |
| POST | `/{provider}/v1/responses/compact` | 用户 API Key | OpenAI Responses compact 代理。 |
| POST | `/{provider}/v1/embeddings` | 用户 API Key | Embeddings 代理。 |
| POST | `/{provider}/v1/images/generations` | 用户 API Key | 图片生成代理。 |
| POST | `/{provider}/v1/images/edits` | 用户 API Key | 图片编辑代理。 |
| GET | `/{provider}/v1/models` | 用户 API Key | 列出指定 Provider 的模型。 |
| GET | `/{provider}/v1/models/{*model_id}` | 用户 API Key | 读取指定 Provider 下的单个模型详情。 |
| GET | `/{provider}/v1beta/models` | 用户 API Key | Gemini `v1beta` 模型列表代理。 |
| POST | `/{provider}/v1beta/{*target}` | 用户 API Key | Gemini `v1beta` 任意目标路径代理。 |

#### Unscoped 路由

这些路由不在路径里写 Provider，而是由模型前缀或模型别名决定目标 Provider。

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| POST | `/v1/messages` | 用户 API Key | Claude 风格消息生成代理，Provider 由模型前缀或别名解析。 |
| POST | `/v1/messages/count_tokens` | 用户 API Key | Claude 风格 token 统计代理，Provider 由模型前缀或别名解析。 |
| POST | `/v1/chat/completions` | 用户 API Key | OpenAI Chat Completions 代理，Provider 由模型前缀或别名解析。 |
| POST | `/v1/responses` | 用户 API Key | OpenAI Responses HTTP 代理，Provider 由模型前缀或别名解析。 |
| POST | `/v1/responses/input_tokens` | 用户 API Key | OpenAI Responses input token 统计代理，Provider 由模型前缀或别名解析。 |
| POST | `/v1/responses/compact` | 用户 API Key | OpenAI Responses compact 代理，Provider 由模型前缀或别名解析。 |
| POST | `/v1/embeddings` | 用户 API Key | Embeddings 代理，Provider 由模型前缀或别名解析。 |
| POST | `/v1/images/generations` | 用户 API Key | 图片生成代理，Provider 由模型前缀或别名解析。 |
| POST | `/v1/images/edits` | 用户 API Key | 图片编辑代理，Provider 由模型前缀或别名解析。 |
| GET | `/v1/models` | 用户 API Key | 列出模型，Provider 由模型前缀或别名解析。 |
| GET | `/v1/models/{*model_id}` | 用户 API Key | 读取单个模型详情，Provider 由模型前缀或别名解析。 |
| GET | `/v1beta/models` | 用户 API Key | Gemini `v1beta` 模型列表代理，Provider 由模型前缀或别名解析。 |
| POST | `/v1beta/{*target}` | 用户 API Key | Gemini `v1beta` 任意目标路径代理，Provider 由模型前缀或别名解析。 |

#### File 路由

文件路由分为 scoped 和 unscoped 两套。scoped 版本通过 `{provider}` 指定 Provider；unscoped 版本要求请求头提供 `X-Provider`。

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| POST | `/{provider}/v1/files` | 用户 API Key | 向指定 Provider 上传文件。 |
| GET | `/{provider}/v1/files` | 用户 API Key | 列出指定 Provider 的文件。 |
| GET | `/{provider}/v1/files/{file_id}` | 用户 API Key | 读取指定文件元数据。 |
| DELETE | `/{provider}/v1/files/{file_id}` | 用户 API Key | 删除指定文件。 |
| GET | `/{provider}/v1/files/{file_id}/content` | 用户 API Key | 获取指定文件内容。 |
| POST | `/v1/files` | 用户 API Key | 上传文件；目标 Provider 由 `X-Provider` 请求头决定。 |
| GET | `/v1/files` | 用户 API Key | 列出文件；目标 Provider 由 `X-Provider` 请求头决定。 |
| GET | `/v1/files/{file_id}` | 用户 API Key | 读取文件元数据；目标 Provider 由 `X-Provider` 请求头决定。 |
| DELETE | `/v1/files/{file_id}` | 用户 API Key | 删除文件；目标 Provider 由 `X-Provider` 请求头决定。 |
| GET | `/v1/files/{file_id}/content` | 用户 API Key | 获取文件内容；目标 Provider 由 `X-Provider` 请求头决定。 |

### Provider WebSocket API

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| GET | `/{provider}/v1/responses` | 用户 API Key | OpenAI Responses WebSocket；Provider 由路径指定，可用 `?model=` 指定初始模型。 |
| GET | `/{provider}/v1beta/models/{*target_live}` | 用户 API Key | Gemini Live WebSocket；`target_live` 形如 `gemini-2.0-flash:streamGenerateContent`。 |
| GET | `/v1/responses` | 用户 API Key | Unscoped OpenAI Responses WebSocket；必须提供 `?model=provider/model` 或可解析的模型别名。 |

### Provider Admin API

这些路由不走 `/admin` 前缀，但仍然要求管理员 Session Token 或管理员用户自己的 API Key。

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| GET | `/{provider}/v1/oauth` | 管理员 Session 或管理员用户 API Key | 启动指定 Provider 的 OAuth 流程。 |
| GET | `/{provider}/v1/oauth/callback` | 管理员 Session 或管理员用户 API Key | 处理指定 Provider 的 OAuth 回调。 |
| GET | `/{provider}/v1/usage` | 管理员 Session 或管理员用户 API Key | 查询指定 Provider 的上游用量 / 配额信息。 |

## Payload 定义

### 通用 JSON 类型

除非特别说明，HTTP 请求体和响应体都使用 JSON。

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

说明：

- 大多数查询 payload 的筛选字段都可以省略；省略等价于 `Scope.All`。
- 由 `OffsetDateTime` 支撑的时间字段会序列化成时间戳字符串。
- 请求日志 / 响应日志中的原始字节字段会序列化成数字数组。

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

#### 健康、重载与全局设置

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

- `GET /admin/health`：无请求体，返回 `HealthResponse`
- `POST /admin/reload`：无请求体，返回 `ReloadResponse`
- `GET /admin/global-settings`：无请求体，返回 `GlobalSettings`
- `POST /admin/global-settings/upsert`：请求体为 `GlobalSettings`，返回 `AckResponse`

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

- `POST /admin/providers/query`：请求体 `ProviderQueryParams`，返回 `ProviderRow[]`
- `POST /admin/providers/upsert`：请求体 `ProviderWrite`，返回 `AckResponse`
- `POST /admin/providers/delete`：请求体 `DeleteProviderPayload`，返回 `AckResponse`
- `POST /admin/providers/batch-upsert`：请求体 `ProviderWrite[]`，返回 `AckResponse`
- `POST /admin/providers/batch-delete`：请求体为 Provider 名称数组 `string[]`，返回 `AckResponse`

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

- `POST /admin/credentials/query`：请求体 `CredentialQueryParams`，返回 `CredentialRow[]`
- `POST /admin/credentials/upsert`：请求体 `UpsertCredentialPayload`，返回 `AckResponse`
- `POST /admin/credentials/delete`：请求体 `DeleteCredentialPayload`，返回 `AckResponse`
- `POST /admin/credentials/batch-upsert`：请求体 `UpsertCredentialPayload[]`，返回 `AckResponse`
- `POST /admin/credentials/batch-delete`：请求体 `DeleteCredentialPayload[]`，返回 `AckResponse`
- `POST /admin/credential-statuses/query`：请求体 `CredentialHealthQuery`，返回 `CredentialHealthRow[]`
- `POST /admin/credential-statuses/update`：请求体 `UpdateCredentialStatusPayload`，返回 `AckResponse`

`/admin/credentials/query` 返回的是引擎内存里的脱敏凭证 JSON，不会原样回传 secret。

#### Models 与 Aliases

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

- `POST /admin/models/query`：请求体 `ModelQueryParams`，返回 `MemoryModelRow[]`
- `POST /admin/models/upsert`：请求体 `ModelWrite`，返回 `AckResponse`
- `POST /admin/models/delete`：请求体 `DeleteModelPayload`，返回 `AckResponse`
- `POST /admin/models/batch-upsert`：请求体 `ModelWrite[]`，返回 `AckResponse`
- `POST /admin/models/batch-delete`：请求体为模型 id 数组 `number[]`，返回 `AckResponse`
- `POST /admin/model-aliases/query`：无请求体，返回 `MemoryModelAliasRow[]`
- `POST /admin/model-aliases/upsert`：请求体 `ModelAliasWrite`，返回 `AckResponse`
- `POST /admin/model-aliases/delete`：请求体 `DeleteModelAliasPayload`，返回 `AckResponse`
- `POST /admin/model-aliases/batch-upsert`：请求体 `ModelAliasWrite[]`，返回 `AckResponse`
- `POST /admin/model-aliases/batch-delete`：请求体 `DeleteModelAliasPayload[]`，返回 `AckResponse`

#### Users 与 Keys

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

- `POST /admin/users/query`：请求体 `UserQueryParams`，返回 `MemoryUserRow[]`
- `POST /admin/users/upsert`：请求体 `UserWrite`，返回 `AckResponse`
- `POST /admin/users/delete`：请求体 `DeleteUserPayload`，返回 `AckResponse`
- `POST /admin/users/batch-upsert`：请求体 `UserWrite[]`，返回 `AckResponse`
- `POST /admin/users/batch-delete`：请求体为用户 id 数组 `number[]`，返回 `AckResponse`
- `POST /admin/user-keys/query`：请求体 `UserKeyQueryParams`，返回 `MemoryUserKeyRow[]`
- `POST /admin/user-keys/generate`：请求体 `GenerateUserKeyPayload`，返回 `GenerateUserKeyResponse`
- `POST /admin/user-keys/delete`：请求体 `DeleteUserKeyPayload`，返回 `AckResponse`
- `POST /admin/user-keys/batch-upsert`：请求体 `BatchGenerateUserKeysPayload`，返回 `BatchGenerateUserKeysResponse`
- `POST /admin/user-keys/batch-delete`：请求体为 user-key id 数组 `number[]`，返回 `AckResponse`

`/admin/user-keys/batch-upsert` 虽然名字叫 batch-upsert，但真实语义是批量生成 key，不接收 `UserKeyWrite[]`。

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

- `POST /admin/user-permissions/query`：请求体 `PermissionQueryParams`，返回 `MemoryPermissionRow[]`
- `POST /admin/user-permissions/upsert`：请求体 `UserModelPermissionWrite`，返回 `AckResponse`
- `POST /admin/user-permissions/delete`：请求体 `DeletePermissionPayload`，返回 `AckResponse`
- `POST /admin/user-permissions/batch-upsert`：请求体 `UserModelPermissionWrite[]`，返回 `AckResponse`
- `POST /admin/user-permissions/batch-delete`：请求体 `DeletePermissionPayload[]`，返回 `AckResponse`

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

- `POST /admin/user-file-permissions/query`：请求体 `FilePermissionQueryParams`，返回 `MemoryFilePermissionRow[]`
- `POST /admin/user-file-permissions/upsert`：请求体 `UserFilePermissionWrite`，返回 `AckResponse`
- `POST /admin/user-file-permissions/delete`：请求体 `DeleteFilePermissionPayload`，返回 `AckResponse`
- `POST /admin/user-file-permissions/batch-upsert`：请求体 `UserFilePermissionWrite[]`，返回 `AckResponse`
- `POST /admin/user-file-permissions/batch-delete`：请求体 `DeleteFilePermissionPayload[]`，返回 `AckResponse`

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

- `POST /admin/user-rate-limits/query`：请求体 `RateLimitQueryParams`，返回 `MemoryRateLimitRow[]`
- `POST /admin/user-rate-limits/upsert`：请求体 `UserRateLimitWrite`，返回 `AckResponse`
- `POST /admin/user-rate-limits/delete`：请求体 `DeleteRateLimitPayload`，返回 `AckResponse`
- `POST /admin/user-rate-limits/batch-upsert`：请求体 `UserRateLimitWrite[]`，返回 `AckResponse`
- `POST /admin/user-rate-limits/batch-delete`：请求体 `DeleteRateLimitPayload[]`，返回 `AckResponse`

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

- `POST /admin/requests/upstream/query`：请求体 `UpstreamRequestQuery`，返回 `UpstreamRequestQueryRow[]`
- `POST /admin/requests/upstream/count`：请求体 `UpstreamRequestQuery`，返回 `CountResponse`
- `POST /admin/requests/upstream/delete`：请求体 `DeleteRequestsPayload`，返回 `AckResponse`
- `POST /admin/requests/upstream/batch-delete`：请求体为 trace id 数组 `number[]`，返回 `AckResponse`
- `POST /admin/requests/downstream/query`：请求体 `DownstreamRequestQuery`，返回 `DownstreamRequestQueryRow[]`
- `POST /admin/requests/downstream/count`：请求体 `DownstreamRequestQuery`，返回 `CountResponse`
- `POST /admin/requests/downstream/delete`：请求体 `DeleteRequestsPayload`，返回 `AckResponse`
- `POST /admin/requests/downstream/batch-delete`：请求体为 trace id 数组 `number[]`，返回 `AckResponse`

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

- `POST /admin/usages/query`：请求体 `UsageQuery`，返回 `UsageQueryRow[]`
- `POST /admin/usages/count`：请求体 `UsageQuery`，返回 `CountResponse`
- `POST /admin/usages/batch-delete`：请求体为 usage trace id 数组 `number[]`，返回 `AckResponse`

向 `/admin/usages/batch-delete` 传空数组会删除全部 usage 记录。

#### Config

`POST /admin/config/export-toml` 无请求体，返回 TOML 文本而不是 JSON。它导出的逻辑结构如下：

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

- `POST /admin/update/check`：无请求体，返回 `UpdateCheckResponse`
- `POST /admin/update`：请求体 `UpdateParams`，返回 `UpdatePerformResponse`

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

- `POST /user/keys/query`：无请求体，返回 `UserKeyRow[]`
- `POST /user/keys/generate`：请求体 `GenerateKeyPayload`，返回 `GenerateKeyResponse`
- `GET /user/quota`：无请求体，返回 `QuotaResponse`
- `POST /user/usages/query`：请求体 `UsageQuery`，返回 `UsageQueryRow[]`；如果传了 `user_id`，服务端也会覆盖成当前 session 用户
- `POST /user/usages/count`：请求体 `UsageQuery`，返回 `CountResponse`；如果传了 `user_id`，服务端也会覆盖成当前 session 用户

### Provider HTTP API

Provider 代理路由不会再包一层 gproxy 自定义 JSON；它们直接接收和返回协议兼容的上游 payload，只是在路由归一化、provider 解析、模型别名解析、权限检查、限流检查和必要时的协议转换之后转发。

精确的 Rust 类型名在 `sdk/gproxy-protocol/README.md` 里，主要映射如下：

| 路由族 | 请求类型 | 响应类型 |
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
| `GET /{provider}/v1/models`, `GET /v1/models` | 协议对应的 model-list 请求 | 协议对应的 model-list 响应 |
| `GET /{provider}/v1/models/{*model_id}`, `GET /v1/models/{*model_id}` | 协议对应的 model-get 请求 | 协议对应的 model-get 响应 |
| `GET /{provider}/v1beta/models`, `GET /v1beta/models` | `GeminiModelListRequest` | `GeminiModelListResponse` |
| `POST /{provider}/v1beta/{*target}`, `POST /v1beta/{*target}` | Gemini 兼容请求体 | Gemini 兼容响应体 |

补充说明：

- scoped 文件路由通过 `{provider}` 选 provider；unscoped 文件路由要求 `X-Provider` 请求头。
- 文件上传使用 `multipart/form-data`。`gproxy-protocol` 当前内建的强类型覆盖是 Claude 风格文件上传：必须有 `file` part，可选 `purpose` part。
- `GET /{provider}/v1/files/{file_id}/content` 和 `GET /v1/files/{file_id}/content` 返回原始文件字节。

### Provider WebSocket API

和 HTTP 代理路由一样，WebSocket 路由直接使用协议原生消息格式：

| 路由族 | 客户端消息类型 | 服务端消息类型 |
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

- `GET /{provider}/v1/oauth`：查询串会作为 `Record<string, string>` 原样转给 provider channel，返回 `OAuthStartResponse`
- `GET /{provider}/v1/oauth/callback`：查询串也会原样透传，返回 `OAuthCallbackResponse`
- `GET /{provider}/v1/usage`：没有本地固定 schema，返回 provider channel 原样给出的 JSON
