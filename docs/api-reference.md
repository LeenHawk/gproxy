# HTTP API 参考 / HTTP API Reference

[中文](#中文) | [English](#english)

---

## 中文

### 鉴权约定

- 管理员路由使用管理员 API Key。
- 用户路由、Provider HTTP 路由和 Provider WebSocket 路由使用用户 API Key。
- API Key 可放在以下任一请求头中：`Authorization: Bearer <key>`、`x-api-key`、`x-goog-api-key`。
- 普通 HTTP 路由请求体上限为 50 MiB；文件路由请求体上限为 500 MiB。

### Login

| 方法 / Method | 路径 / Path | 鉴权 / Auth | 说明 / Description |
| --- | --- | --- | --- |
| POST | `/login` | 无 / None | 用户名 + 密码登录；密码校验通过后返回该用户第一条启用中的 API Key。 / Logs in with username and password; after successful password validation, returns the first enabled API key for that user. |

### Admin API

#### 健康、重载与全局设置 / Health, Reload, and Global Settings

| 方法 / Method | 路径 / Path | 鉴权 / Auth | 说明 / Description |
| --- | --- | --- | --- |
| GET | `/admin/health` | 管理员 API Key / Admin API Key | 返回服务状态、Provider 数量、用户数量和时间戳。 / Returns service status, provider count, user count, and a timestamp. |
| POST | `/admin/reload` | 管理员 API Key / Admin API Key | 从数据库重新装载所有内存缓存。 / Reloads all in-memory caches from the database. |
| GET | `/admin/global-settings` | 管理员 API Key / Admin API Key | 读取当前全局配置。 / Reads the current global configuration. |
| POST | `/admin/global-settings/upsert` | 管理员 API Key / Admin API Key | 更新全局配置；如果 DSN 改变，会重连数据库并重新 bootstrap。 / Updates the global configuration; if the DSN changes, the process reconnects to the database and bootstraps again. |

#### Providers

| 方法 / Method | 路径 / Path | 鉴权 / Auth | 说明 / Description |
| --- | --- | --- | --- |
| POST | `/admin/providers/query` | 管理员 API Key / Admin API Key | 查询 Provider 列表。 / Queries providers. |
| POST | `/admin/providers/upsert` | 管理员 API Key / Admin API Key | 新增或更新单个 Provider。 / Adds or updates one provider. |
| POST | `/admin/providers/delete` | 管理员 API Key / Admin API Key | 删除单个 Provider。 / Deletes one provider. |
| POST | `/admin/providers/batch-upsert` | 管理员 API Key / Admin API Key | 批量新增或更新 Provider。 / Adds or updates providers in batch. |
| POST | `/admin/providers/batch-delete` | 管理员 API Key / Admin API Key | 批量删除 Provider。 / Deletes providers in batch. |

#### Credentials

| 方法 / Method | 路径 / Path | 鉴权 / Auth | 说明 / Description |
| --- | --- | --- | --- |
| POST | `/admin/credentials/query` | 管理员 API Key / Admin API Key | 查询 Provider 凭证。 / Queries provider credentials. |
| POST | `/admin/credentials/upsert` | 管理员 API Key / Admin API Key | 新增或更新单个凭证。 / Adds or updates one credential. |
| POST | `/admin/credentials/delete` | 管理员 API Key / Admin API Key | 删除单个凭证。 / Deletes one credential. |
| POST | `/admin/credentials/batch-upsert` | 管理员 API Key / Admin API Key | 批量新增或更新凭证。 / Adds or updates credentials in batch. |
| POST | `/admin/credentials/batch-delete` | 管理员 API Key / Admin API Key | 批量删除凭证。 / Deletes credentials in batch. |
| POST | `/admin/credential-statuses/query` | 管理员 API Key / Admin API Key | 查询凭证健康状态。 / Queries credential health statuses. |
| POST | `/admin/credential-statuses/update` | 管理员 API Key / Admin API Key | 手动更新凭证健康状态。 / Updates credential health status manually. |

#### Models 与 Aliases / Models and Aliases

| 方法 / Method | 路径 / Path | 鉴权 / Auth | 说明 / Description |
| --- | --- | --- | --- |
| POST | `/admin/models/query` | 管理员 API Key / Admin API Key | 查询模型列表。 / Queries models. |
| POST | `/admin/models/upsert` | 管理员 API Key / Admin API Key | 新增或更新单个模型。 / Adds or updates one model. |
| POST | `/admin/models/delete` | 管理员 API Key / Admin API Key | 删除单个模型。 / Deletes one model. |
| POST | `/admin/models/batch-upsert` | 管理员 API Key / Admin API Key | 批量新增或更新模型。 / Adds or updates models in batch. |
| POST | `/admin/models/batch-delete` | 管理员 API Key / Admin API Key | 批量删除模型。 / Deletes models in batch. |
| POST | `/admin/model-aliases/query` | 管理员 API Key / Admin API Key | 查询模型别名。 / Queries model aliases. |
| POST | `/admin/model-aliases/upsert` | 管理员 API Key / Admin API Key | 新增或更新单个模型别名。 / Adds or updates one model alias. |
| POST | `/admin/model-aliases/delete` | 管理员 API Key / Admin API Key | 删除单个模型别名。 / Deletes one model alias. |
| POST | `/admin/model-aliases/batch-upsert` | 管理员 API Key / Admin API Key | 批量新增或更新模型别名。 / Adds or updates model aliases in batch. |
| POST | `/admin/model-aliases/batch-delete` | 管理员 API Key / Admin API Key | 批量删除模型别名。 / Deletes model aliases in batch. |

#### Users 与 Keys / Users and Keys

| 方法 / Method | 路径 / Path | 鉴权 / Auth | 说明 / Description |
| --- | --- | --- | --- |
| POST | `/admin/users/query` | 管理员 API Key / Admin API Key | 查询用户列表。 / Queries users. |
| POST | `/admin/users/upsert` | 管理员 API Key / Admin API Key | 新增或更新单个用户。 / Adds or updates one user. |
| POST | `/admin/users/delete` | 管理员 API Key / Admin API Key | 删除单个用户。 / Deletes one user. |
| POST | `/admin/users/batch-upsert` | 管理员 API Key / Admin API Key | 批量新增或更新用户。 / Adds or updates users in batch. |
| POST | `/admin/users/batch-delete` | 管理员 API Key / Admin API Key | 批量删除用户。 / Deletes users in batch. |
| POST | `/admin/user-keys/query` | 管理员 API Key / Admin API Key | 查询用户 API Key。 / Queries user API keys. |
| POST | `/admin/user-keys/generate` | 管理员 API Key / Admin API Key | 为指定用户生成新的 API Key。 / Generates a new API key for the specified user. |
| POST | `/admin/user-keys/delete` | 管理员 API Key / Admin API Key | 删除单个用户 API Key。 / Deletes one user API key. |
| POST | `/admin/user-keys/batch-upsert` | 管理员 API Key / Admin API Key | 批量新增或更新用户 API Key。 / Adds or updates user API keys in batch. |
| POST | `/admin/user-keys/batch-delete` | 管理员 API Key / Admin API Key | 批量删除用户 API Key。 / Deletes user API keys in batch. |

#### Permissions

| 方法 / Method | 路径 / Path | 鉴权 / Auth | 说明 / Description |
| --- | --- | --- | --- |
| POST | `/admin/user-permissions/query` | 管理员 API Key / Admin API Key | 查询用户模型权限。 / Queries user model permissions. |
| POST | `/admin/user-permissions/upsert` | 管理员 API Key / Admin API Key | 新增或更新单条模型权限。 / Adds or updates one model permission. |
| POST | `/admin/user-permissions/delete` | 管理员 API Key / Admin API Key | 删除单条模型权限。 / Deletes one model permission. |
| POST | `/admin/user-permissions/batch-upsert` | 管理员 API Key / Admin API Key | 批量新增或更新模型权限。 / Adds or updates model permissions in batch. |
| POST | `/admin/user-permissions/batch-delete` | 管理员 API Key / Admin API Key | 批量删除模型权限。 / Deletes model permissions in batch. |

#### File Permissions

| 方法 / Method | 路径 / Path | 鉴权 / Auth | 说明 / Description |
| --- | --- | --- | --- |
| POST | `/admin/user-file-permissions/query` | 管理员 API Key / Admin API Key | 查询用户文件权限。 / Queries user file permissions. |
| POST | `/admin/user-file-permissions/upsert` | 管理员 API Key / Admin API Key | 新增或更新单条文件权限。 / Adds or updates one file permission. |
| POST | `/admin/user-file-permissions/delete` | 管理员 API Key / Admin API Key | 删除单条文件权限。 / Deletes one file permission. |
| POST | `/admin/user-file-permissions/batch-upsert` | 管理员 API Key / Admin API Key | 批量新增或更新文件权限。 / Adds or updates file permissions in batch. |
| POST | `/admin/user-file-permissions/batch-delete` | 管理员 API Key / Admin API Key | 批量删除文件权限。 / Deletes file permissions in batch. |

#### Rate Limits

| 方法 / Method | 路径 / Path | 鉴权 / Auth | 说明 / Description |
| --- | --- | --- | --- |
| POST | `/admin/user-rate-limits/query` | 管理员 API Key / Admin API Key | 查询用户限流规则。 / Queries user rate-limit rules. |
| POST | `/admin/user-rate-limits/upsert` | 管理员 API Key / Admin API Key | 新增或更新单条限流规则。 / Adds or updates one rate-limit rule. |
| POST | `/admin/user-rate-limits/delete` | 管理员 API Key / Admin API Key | 删除单条限流规则。 / Deletes one rate-limit rule. |
| POST | `/admin/user-rate-limits/batch-upsert` | 管理员 API Key / Admin API Key | 批量新增或更新限流规则。 / Adds or updates rate-limit rules in batch. |
| POST | `/admin/user-rate-limits/batch-delete` | 管理员 API Key / Admin API Key | 批量删除限流规则。 / Deletes rate-limit rules in batch. |

#### Requests

| 方法 / Method | 路径 / Path | 鉴权 / Auth | 说明 / Description |
| --- | --- | --- | --- |
| POST | `/admin/requests/upstream/query` | 管理员 API Key / Admin API Key | 查询上游请求日志。 / Queries upstream request logs. |
| POST | `/admin/requests/upstream/count` | 管理员 API Key / Admin API Key | 统计上游请求日志。 / Counts upstream request logs. |
| POST | `/admin/requests/upstream/delete` | 管理员 API Key / Admin API Key | 删除单条或按条件删除上游请求日志。 / Deletes one upstream request log or deletes by condition. |
| POST | `/admin/requests/upstream/batch-delete` | 管理员 API Key / Admin API Key | 批量删除上游请求日志。 / Deletes upstream request logs in batch. |
| POST | `/admin/requests/downstream/query` | 管理员 API Key / Admin API Key | 查询下游请求日志。 / Queries downstream request logs. |
| POST | `/admin/requests/downstream/count` | 管理员 API Key / Admin API Key | 统计下游请求日志。 / Counts downstream request logs. |
| POST | `/admin/requests/downstream/delete` | 管理员 API Key / Admin API Key | 删除单条或按条件删除下游请求日志。 / Deletes one downstream request log or deletes by condition. |
| POST | `/admin/requests/downstream/batch-delete` | 管理员 API Key / Admin API Key | 批量删除下游请求日志。 / Deletes downstream request logs in batch. |

#### Usages

| 方法 / Method | 路径 / Path | 鉴权 / Auth | 说明 / Description |
| --- | --- | --- | --- |
| POST | `/admin/usages/query` | 管理员 API Key / Admin API Key | 查询 usage 记录。 / Queries usage records. |
| POST | `/admin/usages/count` | 管理员 API Key / Admin API Key | 统计 usage 记录。 / Counts usage records. |
| POST | `/admin/usages/batch-delete` | 管理员 API Key / Admin API Key | 批量删除 usage 记录。 / Deletes usage records in batch. |

#### Config

| 方法 / Method | 路径 / Path | 鉴权 / Auth | 说明 / Description |
| --- | --- | --- | --- |
| POST | `/admin/config/export-toml` | 管理员 API Key / Admin API Key | 导出当前内存 / 配置状态为 TOML。 / Exports the current in-memory and configuration state as TOML. |

#### Update

| 方法 / Method | 路径 / Path | 鉴权 / Auth | 说明 / Description |
| --- | --- | --- | --- |
| POST | `/admin/update/check` | 管理员 API Key / Admin API Key | 检查是否有新版本，并返回下载地址。 / Checks for a new version and returns the download URL. |
| POST | `/admin/update` | 管理员 API Key / Admin API Key | 下载、校验并替换当前可执行文件，然后调度重启。 / Downloads, verifies, and replaces the current executable, then schedules a restart. |

### User API

#### Keys

| 方法 / Method | 路径 / Path | 鉴权 / Auth | 说明 / Description |
| --- | --- | --- | --- |
| POST | `/user/keys/query` | 用户 API Key / User API Key | 查询当前用户自己的 API Key。 / Queries API keys owned by the current user. |
| POST | `/user/keys/generate` | 用户 API Key / User API Key | 为当前用户生成新的 API Key。 / Generates a new API key for the current user. |

#### Quota

| 方法 / Method | 路径 / Path | 鉴权 / Auth | 说明 / Description |
| --- | --- | --- | --- |
| GET | `/user/quota` | 用户 API Key / User API Key | 返回当前用户的总配额、已用成本和剩余预算。 / Returns the current user's total quota, used cost, and remaining budget. |

#### Usages

| 方法 / Method | 路径 / Path | 鉴权 / Auth | 说明 / Description |
| --- | --- | --- | --- |
| POST | `/user/usages/query` | 用户 API Key / User API Key | 查询当前用户的 usage 记录。 / Queries usage records for the current user. |
| POST | `/user/usages/count` | 用户 API Key / User API Key | 统计当前用户的 usage 记录。 / Counts usage records for the current user. |

### Provider HTTP API

#### Scoped 路由 / Scoped Routes

这些路由通过路径中的 `{provider}` 指定目标 Provider，全部走用户鉴权，并经过请求净化、模型别名解析、模型提取、分类和权限 / 限流检查。

| 方法 / Method | 路径 / Path | 鉴权 / Auth | 说明 / Description |
| --- | --- | --- | --- |
| POST | `/{provider}/v1/messages` | 用户 API Key / User API Key | Claude 风格消息生成代理。 / Claude-style message generation proxy. |
| POST | `/{provider}/v1/messages/count-tokens` | 用户 API Key / User API Key | Claude 风格 token 统计代理。 / Claude-style token counting proxy. |
| POST | `/{provider}/v1/chat/completions` | 用户 API Key / User API Key | OpenAI Chat Completions 代理。 / OpenAI Chat Completions proxy. |
| POST | `/{provider}/v1/responses` | 用户 API Key / User API Key | OpenAI Responses HTTP 代理。 / OpenAI Responses HTTP proxy. |
| POST | `/{provider}/v1/responses/input_tokens` | 用户 API Key / User API Key | OpenAI Responses input token 统计代理。 / OpenAI Responses input-token counting proxy. |
| POST | `/{provider}/v1/responses/compact` | 用户 API Key / User API Key | OpenAI Responses compact 代理。 / OpenAI Responses compact proxy. |
| POST | `/{provider}/v1/embeddings` | 用户 API Key / User API Key | Embeddings 代理。 / Embeddings proxy. |
| POST | `/{provider}/v1/images/generations` | 用户 API Key / User API Key | 图片生成代理。 / Image-generation proxy. |
| POST | `/{provider}/v1/images/edits` | 用户 API Key / User API Key | 图片编辑代理。 / Image-editing proxy. |
| GET | `/{provider}/v1/models` | 用户 API Key / User API Key | 列出指定 Provider 的模型。 / Lists models for the specified provider. |
| GET | `/{provider}/v1/models/{*model_id}` | 用户 API Key / User API Key | 读取指定 Provider 下的单个模型详情。 / Reads details for one model under the specified provider. |
| GET | `/{provider}/v1beta/models` | 用户 API Key / User API Key | Gemini `v1beta` 模型列表代理。 / Gemini `v1beta` model-list proxy. |
| POST | `/{provider}/v1beta/{*target}` | 用户 API Key / User API Key | Gemini `v1beta` 任意目标路径代理。 / Gemini `v1beta` arbitrary target-path proxy. |

#### Unscoped 路由 / Unscoped Routes

这些路由不在路径里写 Provider，而是由模型前缀或模型别名决定目标 Provider。

| 方法 / Method | 路径 / Path | 鉴权 / Auth | 说明 / Description |
| --- | --- | --- | --- |
| POST | `/v1/messages` | 用户 API Key / User API Key | Claude 风格消息生成代理，Provider 由模型前缀或别名解析。 / Claude-style message generation proxy; the provider is resolved from the model prefix or alias. |
| POST | `/v1/messages/count_tokens` | 用户 API Key / User API Key | Claude 风格 token 统计代理，Provider 由模型前缀或别名解析。 / Claude-style token counting proxy; the provider is resolved from the model prefix or alias. |
| POST | `/v1/chat/completions` | 用户 API Key / User API Key | OpenAI Chat Completions 代理，Provider 由模型前缀或别名解析。 / OpenAI Chat Completions proxy; the provider is resolved from the model prefix or alias. |
| POST | `/v1/responses` | 用户 API Key / User API Key | OpenAI Responses HTTP 代理，Provider 由模型前缀或别名解析。 / OpenAI Responses HTTP proxy; the provider is resolved from the model prefix or alias. |
| POST | `/v1/responses/input_tokens` | 用户 API Key / User API Key | OpenAI Responses input token 统计代理，Provider 由模型前缀或别名解析。 / OpenAI Responses input-token counting proxy; the provider is resolved from the model prefix or alias. |
| POST | `/v1/responses/compact` | 用户 API Key / User API Key | OpenAI Responses compact 代理，Provider 由模型前缀或别名解析。 / OpenAI Responses compact proxy; the provider is resolved from the model prefix or alias. |
| POST | `/v1/embeddings` | 用户 API Key / User API Key | Embeddings 代理，Provider 由模型前缀或别名解析。 / Embeddings proxy; the provider is resolved from the model prefix or alias. |
| POST | `/v1/images/generations` | 用户 API Key / User API Key | 图片生成代理，Provider 由模型前缀或别名解析。 / Image-generation proxy; the provider is resolved from the model prefix or alias. |
| POST | `/v1/images/edits` | 用户 API Key / User API Key | 图片编辑代理，Provider 由模型前缀或别名解析。 / Image-editing proxy; the provider is resolved from the model prefix or alias. |
| GET | `/v1/models` | 用户 API Key / User API Key | 列出模型，Provider 由模型前缀或别名解析。 / Lists models; the provider is resolved from the model prefix or alias. |
| GET | `/v1/models/{*model_id}` | 用户 API Key / User API Key | 读取单个模型详情，Provider 由模型前缀或别名解析。 / Reads details for a single model; the provider is resolved from the model prefix or alias. |
| GET | `/v1beta/models` | 用户 API Key / User API Key | Gemini `v1beta` 模型列表代理，Provider 由模型前缀或别名解析。 / Gemini `v1beta` model-list proxy; the provider is resolved from the model prefix or alias. |
| POST | `/v1beta/{*target}` | 用户 API Key / User API Key | Gemini `v1beta` 任意目标路径代理，Provider 由模型前缀或别名解析。 / Gemini `v1beta` arbitrary target-path proxy; the provider is resolved from the model prefix or alias. |

#### File 路由 / File Routes

文件路由分为 scoped 和 unscoped 两套。scoped 版本通过 `{provider}` 指定 Provider；unscoped 版本要求请求头提供 `X-Provider`。

| 方法 / Method | 路径 / Path | 鉴权 / Auth | 说明 / Description |
| --- | --- | --- | --- |
| POST | `/{provider}/v1/files` | 用户 API Key / User API Key | 向指定 Provider 上传文件。 / Uploads a file to the specified provider. |
| GET | `/{provider}/v1/files` | 用户 API Key / User API Key | 列出指定 Provider 的文件。 / Lists files for the specified provider. |
| GET | `/{provider}/v1/files/{file_id}` | 用户 API Key / User API Key | 读取指定文件元数据。 / Reads metadata for the specified file. |
| DELETE | `/{provider}/v1/files/{file_id}` | 用户 API Key / User API Key | 删除指定文件。 / Deletes the specified file. |
| GET | `/{provider}/v1/files/{file_id}/content` | 用户 API Key / User API Key | 获取指定文件内容。 / Retrieves content for the specified file. |
| POST | `/v1/files` | 用户 API Key / User API Key | 上传文件；目标 Provider 由 `X-Provider` 请求头决定。 / Uploads a file; the target provider is chosen via the `X-Provider` header. |
| GET | `/v1/files` | 用户 API Key / User API Key | 列出文件；目标 Provider 由 `X-Provider` 请求头决定。 / Lists files; the target provider is chosen via the `X-Provider` header. |
| GET | `/v1/files/{file_id}` | 用户 API Key / User API Key | 读取文件元数据；目标 Provider 由 `X-Provider` 请求头决定。 / Reads file metadata; the target provider is chosen via the `X-Provider` header. |
| DELETE | `/v1/files/{file_id}` | 用户 API Key / User API Key | 删除文件；目标 Provider 由 `X-Provider` 请求头决定。 / Deletes a file; the target provider is chosen via the `X-Provider` header. |
| GET | `/v1/files/{file_id}/content` | 用户 API Key / User API Key | 获取文件内容；目标 Provider 由 `X-Provider` 请求头决定。 / Retrieves file content; the target provider is chosen via the `X-Provider` header. |

### Provider WebSocket API

| 方法 / Method | 路径 / Path | 鉴权 / Auth | 说明 / Description |
| --- | --- | --- | --- |
| GET | `/{provider}/v1/responses` | 用户 API Key / User API Key | OpenAI Responses WebSocket；Provider 由路径指定，可用 `?model=` 指定初始模型。 / OpenAI Responses WebSocket; the provider is specified in the path, and `?model=` can be used to choose the initial model. |
| GET | `/{provider}/v1beta/models/{*target_live}` | 用户 API Key / User API Key | Gemini Live WebSocket；`target_live` 形如 `gemini-2.0-flash:streamGenerateContent`。 / Gemini Live WebSocket; `target_live` looks like `gemini-2.0-flash:streamGenerateContent`. |
| GET | `/v1/responses` | 用户 API Key / User API Key | Unscoped OpenAI Responses WebSocket；必须提供 `?model=provider/model` 或可解析的模型别名。 / Unscoped OpenAI Responses WebSocket; requires `?model=provider/model` or a resolvable model alias. |

### Provider Admin API

这些路由不走 `/admin` 前缀，但仍然使用管理员 API Key。

| 方法 / Method | 路径 / Path | 鉴权 / Auth | 说明 / Description |
| --- | --- | --- | --- |
| GET | `/{provider}/v1/oauth` | 管理员 API Key / Admin API Key | 启动指定 Provider 的 OAuth 流程。 / Starts the OAuth flow for the specified provider. |
| GET | `/{provider}/v1/oauth/callback` | 管理员 API Key / Admin API Key | 处理指定 Provider 的 OAuth 回调。 / Handles the OAuth callback for the specified provider. |
| GET | `/{provider}/v1/usage` | 管理员 API Key / Admin API Key | 查询指定 Provider 的上游用量 / 配额信息。 / Queries upstream usage or quota information for the specified provider. |

---

## English

### Authentication Conventions

- Admin routes use the admin API key.
- User routes, Provider HTTP routes, and Provider WebSocket routes use a user API key.
- API keys may be sent in any of these headers: `Authorization: Bearer <key>`, `x-api-key`, or `x-goog-api-key`.
- Regular HTTP routes accept request bodies up to 50 MiB, while file routes accept bodies up to 500 MiB.

### Login

See the shared bilingual table above for the `POST /login` contract.

### Admin API

#### Health, Reload, and Global Settings

See the shared bilingual table above for the admin health, reload, and global-settings endpoints.

#### Providers

See the shared bilingual table above for provider query, upsert, delete, and batch operations.

#### Credentials

See the shared bilingual table above for credential query, mutation, and health-status operations.

#### Models and Aliases

See the shared bilingual table above for model and model-alias operations.

#### Users and Keys

See the shared bilingual table above for user and user-key operations.

#### Permissions

See the shared bilingual table above for model-permission routes.

#### File Permissions

See the shared bilingual table above for file-permission routes.

#### Rate Limits

See the shared bilingual table above for rate-limit routes.

#### Requests

See the shared bilingual table above for upstream and downstream request-log routes.

#### Usages

See the shared bilingual table above for usage query, count, and batch-delete routes.

#### Config

See the shared bilingual table above for the config export route.

#### Update

See the shared bilingual table above for update checking and binary replacement routes.

### User API

#### Keys

See the shared bilingual table above for user key routes.

#### Quota

See the shared bilingual table above for the user quota route.

#### Usages

See the shared bilingual table above for user usage query and count routes.

### Provider HTTP API

#### Scoped Routes

The shared table above lists the routes that target a provider explicitly through `{provider}` in the path. They all use user authentication and run through request sanitization, model-alias resolution, model extraction, classification, and permission or rate-limit checks.

#### Unscoped Routes

The shared table above lists the routes that omit the provider in the path and instead resolve the target provider from the model prefix or model alias.

#### File Routes

The shared table above lists both scoped and unscoped file routes. Scoped routes select the provider through `{provider}`, while unscoped routes require the `X-Provider` request header.

### Provider WebSocket API

See the shared bilingual table above for the OpenAI Responses and Gemini Live WebSocket routes.

### Provider Admin API

These routes do not use the `/admin` prefix, but they still require the admin API key. See the shared bilingual table above for the OAuth and provider-usage endpoints.
