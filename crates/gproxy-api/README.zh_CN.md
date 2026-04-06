# HTTP API 参考

### 鉴权约定

- 管理员路由接受 `/admin/login` 返回的管理员 Session Token，或管理员用户自己的 API Key。
- `/user/*` 路由接受 `/login` 返回的普通用户 Session Token。
- Provider HTTP 路由和 Provider WebSocket 路由使用用户 API Key。
- Session Token 和 API Key 都可以放在以下任一请求头中：`Authorization: Bearer <token>`、`x-api-key`、`x-goog-api-key`。
- 普通 HTTP 路由请求体上限为 50 MiB；文件路由请求体上限为 500 MiB。

### Login

| 方法 | 路径 | 鉴权 | 说明 |
| --- | --- | --- | --- |
| POST | `/login` | 无 | 非管理员用户使用用户名 + 密码登录，返回用户 Session Token。 |
| POST | `/admin/login` | 无 | 管理员用户使用用户名 + 密码登录，返回管理员 Session Token。 |

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
