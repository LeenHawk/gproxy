# 当前已实现路由清单

> 基于 `crates/gproxy-api/src/` 实际注册的路由整理，截至 2026-04-05。

---

## 顶层路由组装

**入口：** `crates/gproxy-api/src/router.rs` → `api_router()`

```
Router
├── POST /login                      ← 无鉴权，50 MB body limit
├── /admin/*                         ← require_admin_middleware，50 MB body limit
├── /user/*                          ← require_user_middleware，50 MB body limit
├── provider HTTP 路由 (scoped + unscoped)  ← require_user_middleware + 分类中间件
├── provider WS 路由                  ← require_user_middleware + sanitize
└── provider admin 路由 (OAuth/Usage) ← require_admin_middleware
```

**全局中间件：** `CorsLayer::permissive()` 应用于所有路由。

---

## 1. 登录路由

| 方法 | 路径 | 鉴权 | Handler | 备注 |
|------|------|------|---------|------|
| POST | `/login` | 无 | `login::login` | 用户名密码登录，返回 API key |

**来源：** `router.rs` 直接注册。

---

## 2. Admin 路由 (`/admin/*`)

**鉴权：** `require_admin_middleware` — 常量时间比较 `admin_key`。  
**来源：** `crates/gproxy-api/src/admin/mod.rs` → `router()`

### 2.1 系统管理

| 方法 | 路径 | Handler | 备注 |
|------|------|---------|------|
| GET | `/admin/health` | `health::health` | 健康检查 |
| POST | `/admin/reload` | `reload::reload` | 从 DB 重新加载全部内存状态 |

### 2.2 全局设置

| 方法 | 路径 | Handler |
|------|------|---------|
| GET | `/admin/global-settings` | `settings::get_global_settings` |
| POST | `/admin/global-settings/upsert` | `settings::upsert_global_settings` |

### 2.3 Provider 管理

| 方法 | 路径 | Handler |
|------|------|---------|
| POST | `/admin/providers/query` | `providers::query_providers` |
| POST | `/admin/providers/upsert` | `providers::upsert_provider` |
| POST | `/admin/providers/delete` | `providers::delete_provider` |
| POST | `/admin/providers/batch-upsert` | `providers::batch_upsert_providers` |
| POST | `/admin/providers/batch-delete` | `providers::batch_delete_providers` |

### 2.4 Credential 管理

| 方法 | 路径 | Handler |
|------|------|---------|
| POST | `/admin/credentials/query` | `credentials::query_credentials` |
| POST | `/admin/credentials/upsert` | `credentials::upsert_credential` |
| POST | `/admin/credentials/delete` | `credentials::delete_credential` |
| POST | `/admin/credentials/batch-upsert` | `credentials::batch_upsert_credentials` |
| POST | `/admin/credentials/batch-delete` | `credentials::batch_delete_credentials` |
| POST | `/admin/credential-statuses/query` | `credentials::query_credential_statuses` |
| POST | `/admin/credential-statuses/update` | `credentials::update_credential_status` |

### 2.5 Model 管理

| 方法 | 路径 | Handler |
|------|------|---------|
| POST | `/admin/models/query` | `models::query_models` |
| POST | `/admin/models/upsert` | `models::upsert_model` |
| POST | `/admin/models/delete` | `models::delete_model` |
| POST | `/admin/models/batch-upsert` | `models::batch_upsert_models` |
| POST | `/admin/models/batch-delete` | `models::batch_delete_models` |

### 2.6 Model Alias 管理

| 方法 | 路径 | Handler |
|------|------|---------|
| POST | `/admin/model-aliases/query` | `models::query_model_aliases` |
| POST | `/admin/model-aliases/upsert` | `models::upsert_model_alias` |
| POST | `/admin/model-aliases/delete` | `models::delete_model_alias` |
| POST | `/admin/model-aliases/batch-upsert` | `models::batch_upsert_model_aliases` |
| POST | `/admin/model-aliases/batch-delete` | `models::batch_delete_model_aliases` |

### 2.7 User 管理

| 方法 | 路径 | Handler |
|------|------|---------|
| POST | `/admin/users/query` | `users::query_users` |
| POST | `/admin/users/upsert` | `users::upsert_user` |
| POST | `/admin/users/delete` | `users::delete_user` |
| POST | `/admin/users/batch-upsert` | `users::batch_upsert_users` |
| POST | `/admin/users/batch-delete` | `users::batch_delete_users` |

### 2.8 User Key 管理

| 方法 | 路径 | Handler |
|------|------|---------|
| POST | `/admin/user-keys/query` | `users::query_user_keys` |
| POST | `/admin/user-keys/generate` | `users::generate_user_key` |
| POST | `/admin/user-keys/delete` | `users::delete_user_key` |
| POST | `/admin/user-keys/batch-upsert` | `users::batch_upsert_user_keys` |
| POST | `/admin/user-keys/batch-delete` | `users::batch_delete_user_keys` |

### 2.9 User Permission 管理

| 方法 | 路径 | Handler |
|------|------|---------|
| POST | `/admin/user-permissions/query` | `permissions::query_permissions` |
| POST | `/admin/user-permissions/upsert` | `permissions::upsert_permission` |
| POST | `/admin/user-permissions/delete` | `permissions::delete_permission` |
| POST | `/admin/user-permissions/batch-upsert` | `permissions::batch_upsert_permissions` |
| POST | `/admin/user-permissions/batch-delete` | `permissions::batch_delete_permissions` |

### 2.10 User File Permission 管理

| 方法 | 路径 | Handler |
|------|------|---------|
| POST | `/admin/user-file-permissions/query` | `file_permissions::query_file_permissions` |
| POST | `/admin/user-file-permissions/upsert` | `file_permissions::upsert_file_permission` |
| POST | `/admin/user-file-permissions/delete` | `file_permissions::delete_file_permission` |
| POST | `/admin/user-file-permissions/batch-upsert` | `file_permissions::batch_upsert_file_permissions` |
| POST | `/admin/user-file-permissions/batch-delete` | `file_permissions::batch_delete_file_permissions` |

### 2.11 User Rate Limit 管理

| 方法 | 路径 | Handler |
|------|------|---------|
| POST | `/admin/user-rate-limits/query` | `rate_limits::query_rate_limits` |
| POST | `/admin/user-rate-limits/upsert` | `rate_limits::upsert_rate_limit` |
| POST | `/admin/user-rate-limits/delete` | `rate_limits::delete_rate_limit` |
| POST | `/admin/user-rate-limits/batch-upsert` | `rate_limits::batch_upsert_rate_limits` |
| POST | `/admin/user-rate-limits/batch-delete` | `rate_limits::batch_delete_rate_limits` |

### 2.12 请求日志

| 方法 | 路径 | Handler |
|------|------|---------|
| POST | `/admin/requests/upstream/query` | `requests::query_upstream_requests` |
| POST | `/admin/requests/upstream/count` | `requests::count_upstream_requests` |
| POST | `/admin/requests/upstream/delete` | `requests::delete_upstream_requests` |
| POST | `/admin/requests/upstream/batch-delete` | `requests::batch_delete_upstream_requests` |
| POST | `/admin/requests/downstream/query` | `requests::query_downstream_requests` |
| POST | `/admin/requests/downstream/count` | `requests::count_downstream_requests` |
| POST | `/admin/requests/downstream/delete` | `requests::delete_downstream_requests` |
| POST | `/admin/requests/downstream/batch-delete` | `requests::batch_delete_downstream_requests` |

### 2.13 Usage 日志

| 方法 | 路径 | Handler |
|------|------|---------|
| POST | `/admin/usages/query` | `usages::query_usages` |
| POST | `/admin/usages/count` | `usages::count_usages` |
| POST | `/admin/usages/batch-delete` | `usages::batch_delete_usages` |

### 2.14 配置导出 & 自更新

| 方法 | 路径 | Handler |
|------|------|---------|
| POST | `/admin/config/export-toml` | `config_toml::export_toml` |
| POST | `/admin/update/check` | `update::check_update` |
| POST | `/admin/update` | `update::perform_update` |

**Admin 路由合计：约 62 条。**

---

## 3. User 路由 (`/user/*`)

**鉴权：** `require_user_middleware` — 校验用户 API key。  
**来源：** `crates/gproxy-api/src/user/mod.rs` → `router()`

| 方法 | 路径 | Handler | 备注 |
|------|------|---------|------|
| POST | `/user/keys/query` | `keys::query_keys` | 查询当前用户的 API key |
| POST | `/user/keys/generate` | `keys::generate_key` | 生成新 API key |
| GET | `/user/quota` | `quota::get_quota` | 查询配额余额 |
| POST | `/user/usages/query` | `usages::query_usages` | 查询用量记录 |
| POST | `/user/usages/count` | `usages::count_usages` | 统计用量 |

**User 路由合计：5 条。**

---

## 4. Provider HTTP 代理路由

**来源：** `crates/gproxy-api/src/provider/mod.rs` → `router()`

### 中间件栈（HTTP 路由，从外到内执行）

1. `RequestBodyLimitLayer` — 50 MB（普通）/ 500 MB（文件）
2. `require_user_middleware` — 用户鉴权
3. `classify_middleware` — 推断 `(OperationFamily, ProtocolKind)`
4. `request_model_middleware` — 从 body 提取 model
5. `model_alias_middleware` — 解析模型别名
6. `sanitize_middleware` — 请求清理

### 4.1 Provider-Scoped 非文件路由

路径格式：`/{provider}/v1/...`，provider 由路径显式指定。

| 方法 | 路径 | Handler | 说明 |
|------|------|---------|------|
| POST | `/{provider}/v1/messages` | `handler::proxy` | Claude Messages |
| POST | `/{provider}/v1/messages/count-tokens` | `handler::proxy` | Claude Count Tokens |
| POST | `/{provider}/v1/chat/completions` | `handler::proxy` | OpenAI Chat Completions |
| POST | `/{provider}/v1/responses` | `handler::proxy` | OpenAI Responses |
| POST | `/{provider}/v1/responses/input_tokens` | `handler::proxy` | OpenAI Input Tokens |
| POST | `/{provider}/v1/responses/compact` | `handler::proxy` | OpenAI Compact Responses |
| POST | `/{provider}/v1/embeddings` | `handler::proxy` | Embeddings |
| POST | `/{provider}/v1/images/generations` | `handler::proxy` | Image 生成 |
| POST | `/{provider}/v1/images/edits` | `handler::proxy` | Image 编辑 |
| GET | `/{provider}/v1/models` | `handler::proxy` | Model 列表 |
| GET | `/{provider}/v1/models/{*model_id}` | `handler::proxy` | Model 详情 |
| GET | `/{provider}/v1beta/models` | `handler::proxy` | Gemini model 列表 |
| POST | `/{provider}/v1beta/{*target}` | `handler::proxy` | Gemini 通用代理 |

### 4.2 Unscoped 非文件路由

路径格式：`/v1/...`，provider 从 model 前缀或 alias 推断。

| 方法 | 路径 | Handler | 说明 |
|------|------|---------|------|
| POST | `/v1/messages` | `handler::proxy_unscoped` | Claude Messages |
| POST | `/v1/messages/count_tokens` | `handler::proxy_unscoped` | Claude Count Tokens |
| POST | `/v1/chat/completions` | `handler::proxy_unscoped` | OpenAI Chat Completions |
| POST | `/v1/responses` | `handler::proxy_unscoped` | OpenAI Responses |
| POST | `/v1/responses/input_tokens` | `handler::proxy_unscoped` | OpenAI Input Tokens |
| POST | `/v1/responses/compact` | `handler::proxy_unscoped` | OpenAI Compact Responses |
| POST | `/v1/embeddings` | `handler::proxy_unscoped` | Embeddings |
| POST | `/v1/images/generations` | `handler::proxy_unscoped` | Image 生成 |
| POST | `/v1/images/edits` | `handler::proxy_unscoped` | Image 编辑 |
| GET | `/v1/models` | `handler::proxy_unscoped` | 聚合所有已授权 provider 的 model 列表 |
| GET | `/v1/models/{*model_id}` | `handler::proxy_unscoped` | Model 详情 |
| GET | `/v1beta/models` | `handler::proxy_unscoped` | Gemini model 列表 |
| POST | `/v1beta/{*target}` | `handler::proxy_unscoped` | Gemini 通用代理 |

### 4.3 Provider-Scoped 文件路由

Body limit 为 500 MB。

| 方法 | 路径 | Handler | 文件 API | 说明 |
|------|------|---------|---------|------|
| POST | `/{provider}/v1/files` | `handler::proxy` | 是 | 上传文件 |
| GET | `/{provider}/v1/files` | `handler::proxy` | 是 | 列出文件 |
| GET | `/{provider}/v1/files/{file_id}` | `handler::proxy` | 是 | 获取元数据 |
| DELETE | `/{provider}/v1/files/{file_id}` | `handler::proxy` | 是 | 删除文件 |
| GET | `/{provider}/v1/files/{file_id}/content` | `handler::proxy` | 是 | 下载内容 |

### 4.4 Unscoped 文件路由

Provider 从 `X-Provider` header 获取。

| 方法 | 路径 | Handler | 文件 API | 说明 |
|------|------|---------|---------|------|
| POST | `/v1/files` | `handler::proxy_unscoped_files` | 是 | 上传文件 |
| GET | `/v1/files` | `handler::proxy_unscoped_files` | 是 | 列出文件 |
| GET | `/v1/files/{file_id}` | `handler::proxy_unscoped_files` | 是 | 获取元数据 |
| DELETE | `/v1/files/{file_id}` | `handler::proxy_unscoped_files` | 是 | 删除文件 |
| GET | `/v1/files/{file_id}/content` | `handler::proxy_unscoped_files` | 是 | 下载内容 |

**Provider HTTP 路由合计：36 条。**

---

## 5. Provider WebSocket 路由

### 中间件栈（WS 路由）

1. `require_user_middleware` — 用户鉴权
2. `sanitize_middleware` — 请求清理

注意：WS 路由**不经过** classify / model_alias / request_model 中间件。

| 方法 | 路径 | Handler | WebSocket | Scoped | 说明 |
|------|------|---------|-----------|--------|------|
| GET | `/{provider}/v1/responses` | `websocket::openai_responses_ws` | 是 | 是 | OpenAI Responses WS |
| GET | `/{provider}/v1beta/models/{*target_live}` | `websocket::gemini_live` | 是 | 是 | Gemini Live WS |
| GET | `/v1/responses` | `websocket::openai_responses_ws_unscoped` | 是 | 否 | OpenAI Responses WS（需 `?model=`） |

**WS 协议桥接实现（`ws_bridge.rs`）：**
- `PassthroughBridge` — 同协议透传，仅提取 usage
- `OpenAiToGeminiBridge` — OpenAI WS 客户端 ↔ Gemini Live 上游
- `GeminiToOpenAiBridge` — Gemini Live 客户端 ↔ OpenAI WS 上游

**Provider WS 路由合计：3 条。**

---

## 6. Provider Admin 路由（OAuth & Usage）

**鉴权：** `require_admin_middleware`  
**来源：** `provider/mod.rs` 中的 `provider_admin_router`

| 方法 | 路径 | Handler | 说明 |
|------|------|---------|------|
| GET | `/{provider}/v1/oauth` | `oauth::oauth_start` | 发起 OAuth 授权流程 |
| GET | `/{provider}/v1/oauth/callback` | `oauth::oauth_callback` | OAuth 回调 |
| GET | `/{provider}/v1/usage` | `oauth::upstream_usage` | 查询上游 provider 的用量/配额 |

**Provider Admin 路由合计：3 条。**

---

## 鉴权方式总结

| 鉴权方式 | Header 来源 | 使用位置 |
|----------|-------------|---------|
| Admin Key | `Authorization: Bearer <key>` / `x-api-key` / `x-goog-api-key` | `/admin/*`，Provider OAuth/Usage |
| User API Key | 同上 | `/user/*`，Provider HTTP/WS |
| 无鉴权 | — | `POST /login` |

**说明：** 三种 header 格式按优先级尝试，`auth.rs` 中 `extract_api_key()` 统一处理。

---

## 路由总数统计

| 分组 | 路由数 |
|------|--------|
| 登录 | 1 |
| Admin | ~62 |
| User | 5 |
| Provider HTTP（scoped + unscoped + 文件） | 36 |
| Provider WebSocket | 3 |
| Provider Admin（OAuth/Usage） | 3 |
| **合计** | **~110** |
