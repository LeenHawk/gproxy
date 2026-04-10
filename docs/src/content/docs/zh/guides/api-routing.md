---
title: API 路由参考
description: GPROXY v1 完整路由参考——所有方法、路径、鉴权要求和协议。
---

## 错误格式

所有错误统一返回 JSON：

```json
{"error": "human-readable message"}
```

HTTP 状态码遵循标准语义（400、401、403、404、429、500、503）。

## 鉴权头提取

GPROXY 按以下顺序检查凭证，使用第一个非空值：

1. `Authorization: Bearer <token>` 请求头
2. `x-api-key` 请求头
3. `x-goog-api-key` 请求头
4. `?key=<value>` 查询参数（Gemini 原生客户端）

Session token（通过 `/login` 获取）以 `sess-` 开头，仅对 `/admin/*` 和 `/user/*` 路由有效。

API key 对 provider 代理路由和 admin 路由（需属于管理员用户）有效。

## 入口路由

| Method | Path | 说明 |
|---|---|---|
| `GET` | `/` | 重定向（308）到 `/console/login` |
| `GET` | `/console` | 控制台 SPA（返回 `index.html`） |
| `GET` | `/console/{*path}` | 控制台 SPA（所有子路由、静态资源） |
| `POST` | `/login` | 密码登录，返回 session token |

### POST `/login`

请求：

```json
{"username": "admin", "password": "..."}
```

响应：

```json
{
  "user_id": 1,
  "session_token": "sess-...",
  "expires_in_secs": 86400,
  "is_admin": true
}
```

## Admin 路由（`/admin/*`）

需要管理员鉴权：管理员用户的 session token，或管理员用户拥有的 API key。非管理员 key 返回 `403 forbidden`。

### 系统

| Method | Path | 说明 |
|---|---|---|
| `GET` | `/admin/health` | 健康检查 |
| `POST` | `/admin/reload` | 从数据库重新加载所有内存缓存 |

### 全局设置

| Method | Path | 说明 |
|---|---|---|
| `GET` | `/admin/global-settings` | 读取全局设置 |
| `POST` | `/admin/global-settings/upsert` | 更新全局设置 |

### Provider

| Method | Path | 说明 |
|---|---|---|
| `POST` | `/admin/providers/query` | 查询 provider |
| `POST` | `/admin/providers/default-dispatch` | 获取 channel 类型的默认 dispatch 表 |
| `POST` | `/admin/providers/upsert` | 创建或更新 provider |
| `POST` | `/admin/providers/delete` | 删除 provider |
| `POST` | `/admin/providers/batch-upsert` | 批量创建/更新 provider |
| `POST` | `/admin/providers/batch-delete` | 批量删除 provider |

### Credential

| Method | Path | 说明 |
|---|---|---|
| `POST` | `/admin/credentials/query` | 查询凭证 |
| `POST` | `/admin/credentials/upsert` | 创建或更新凭证 |
| `POST` | `/admin/credentials/delete` | 删除凭证 |
| `POST` | `/admin/credentials/batch-upsert` | 批量创建/更新凭证 |
| `POST` | `/admin/credentials/batch-delete` | 批量删除凭证 |

### Credential 状态

| Method | Path | 说明 |
|---|---|---|
| `POST` | `/admin/credential-statuses/query` | 查询凭证健康状态 |
| `POST` | `/admin/credential-statuses/update` | 更新凭证健康状态 |

### Model

| Method | Path | 说明 |
|---|---|---|
| `POST` | `/admin/models/query` | 查询模型 |
| `POST` | `/admin/models/upsert` | 创建或更新模型 |
| `POST` | `/admin/models/delete` | 删除模型 |
| `POST` | `/admin/models/batch-upsert` | 批量创建/更新模型 |
| `POST` | `/admin/models/batch-delete` | 批量删除模型 |

### Model 别名

| Method | Path | 说明 |
|---|---|---|
| `POST` | `/admin/model-aliases/query` | 查询模型别名 |
| `POST` | `/admin/model-aliases/upsert` | 创建或更新模型别名 |
| `POST` | `/admin/model-aliases/delete` | 删除模型别名 |
| `POST` | `/admin/model-aliases/batch-upsert` | 批量创建/更新模型别名 |
| `POST` | `/admin/model-aliases/batch-delete` | 批量删除模型别名 |

### 用户

| Method | Path | 说明 |
|---|---|---|
| `POST` | `/admin/users/query` | 查询用户 |
| `POST` | `/admin/users/upsert` | 创建或更新用户 |
| `POST` | `/admin/users/delete` | 删除用户 |
| `POST` | `/admin/users/batch-upsert` | 批量创建/更新用户 |
| `POST` | `/admin/users/batch-delete` | 批量删除用户 |

### 用户 Key

| Method | Path | 说明 |
|---|---|---|
| `POST` | `/admin/user-keys/query` | 查询用户 API key |
| `POST` | `/admin/user-keys/generate` | 为用户生成新 API key |
| `POST` | `/admin/user-keys/update-enabled` | 启用或禁用用户 key |
| `POST` | `/admin/user-keys/delete` | 删除用户 key |
| `POST` | `/admin/user-keys/batch-upsert` | 批量创建/更新用户 key |
| `POST` | `/admin/user-keys/batch-delete` | 批量删除用户 key |

### 用户配额

| Method | Path | 说明 |
|---|---|---|
| `POST` | `/admin/user-quotas/query` | 查询用户配额 |
| `POST` | `/admin/user-quotas/upsert` | 创建或更新用户配额 |

### 用户权限

| Method | Path | 说明 |
|---|---|---|
| `POST` | `/admin/user-permissions/query` | 查询用户权限 |
| `POST` | `/admin/user-permissions/upsert` | 创建或更新用户权限 |
| `POST` | `/admin/user-permissions/delete` | 删除用户权限 |
| `POST` | `/admin/user-permissions/batch-upsert` | 批量创建/更新用户权限 |
| `POST` | `/admin/user-permissions/batch-delete` | 批量删除用户权限 |

### 用户文件权限

| Method | Path | 说明 |
|---|---|---|
| `POST` | `/admin/user-file-permissions/query` | 查询用户文件权限 |
| `POST` | `/admin/user-file-permissions/upsert` | 创建或更新用户文件权限 |
| `POST` | `/admin/user-file-permissions/delete` | 删除用户文件权限 |
| `POST` | `/admin/user-file-permissions/batch-upsert` | 批量创建/更新用户文件权限 |
| `POST` | `/admin/user-file-permissions/batch-delete` | 批量删除用户文件权限 |

### 用户速率限制

| Method | Path | 说明 |
|---|---|---|
| `POST` | `/admin/user-rate-limits/query` | 查询用户速率限制 |
| `POST` | `/admin/user-rate-limits/upsert` | 创建或更新用户速率限制 |
| `POST` | `/admin/user-rate-limits/delete` | 删除用户速率限制 |
| `POST` | `/admin/user-rate-limits/batch-upsert` | 批量创建/更新用户速率限制 |
| `POST` | `/admin/user-rate-limits/batch-delete` | 批量删除用户速率限制 |

### 请求日志（审计）

| Method | Path | 说明 |
|---|---|---|
| `POST` | `/admin/requests/upstream/query` | 查询上游请求日志 |
| `POST` | `/admin/requests/upstream/count` | 统计上游请求日志数量 |
| `POST` | `/admin/requests/upstream/clear` | 清除上游请求体（保留元数据） |
| `POST` | `/admin/requests/upstream/delete` | 删除上游请求日志 |
| `POST` | `/admin/requests/upstream/batch-delete` | 批量删除上游请求日志 |
| `POST` | `/admin/requests/downstream/query` | 查询下游请求日志 |
| `POST` | `/admin/requests/downstream/count` | 统计下游请求日志数量 |
| `POST` | `/admin/requests/downstream/clear` | 清除下游请求体（保留元数据） |
| `POST` | `/admin/requests/downstream/delete` | 删除下游请求日志 |
| `POST` | `/admin/requests/downstream/batch-delete` | 批量删除下游请求日志 |

### 用量

| Method | Path | 说明 |
|---|---|---|
| `POST` | `/admin/usages/query` | 查询用量记录 |
| `POST` | `/admin/usages/count` | 统计用量记录数量 |
| `POST` | `/admin/usages/summary` | 聚合用量汇总 |
| `POST` | `/admin/usages/batch-delete` | 批量删除用量记录 |

### 配置导出

| Method | Path | 说明 |
|---|---|---|
| `POST` | `/admin/config/export-toml` | 导出当前配置为 TOML |

### 自更新

| Method | Path | 说明 |
|---|---|---|
| `POST` | `/admin/update/check` | 检查可用更新 |
| `POST` | `/admin/update` | 执行自更新 |

## User 路由（`/user/*`）

需要 session token（通过 `/login` 获取）。任何已认证用户均可访问，不限于管理员。API key 在这些路由上不被接受——防止泄露的推理 key 被用于生成新 key 或枚举已有 key。

| Method | Path | 说明 |
|---|---|---|
| `POST` | `/user/keys/query` | 查询当前用户的 API key |
| `POST` | `/user/keys/generate` | 生成新 API key |
| `POST` | `/user/keys/update-enabled` | 启用或禁用自己的 key |
| `POST` | `/user/keys/delete` | 删除自己的 key |
| `GET` | `/user/quota` | 获取当前用户配额 |
| `POST` | `/user/usages/query` | 查询当前用户的用量记录 |
| `POST` | `/user/usages/count` | 统计当前用户的用量记录 |
| `POST` | `/user/usages/summary` | 当前用户的聚合用量汇总 |

## Provider Scoped 路由（`/{provider}/v1/*`）

需要 API key 鉴权。URL 中的 `{provider}` 路径段决定使用哪个 provider。请求体中的 model 字段无需加 provider 前缀。

### 推理

| Method | Path | 说明 |
|---|---|---|
| `POST` | `/{provider}/v1/messages` | Claude Messages API |
| `POST` | `/{provider}/v1/messages/count-tokens` | Claude token 计数 |
| `POST` | `/{provider}/v1/chat/completions` | OpenAI Chat Completions |
| `POST` | `/{provider}/v1/responses` | OpenAI Responses API |
| `POST` | `/{provider}/v1/responses/input_tokens` | OpenAI Responses 输入 token 计数 |
| `POST` | `/{provider}/v1/responses/compact` | OpenAI Responses compact 模式 |
| `POST` | `/{provider}/v1/embeddings` | Embeddings |
| `POST` | `/{provider}/v1/images/generations` | 图像生成 |
| `POST` | `/{provider}/v1/images/edits` | 图像编辑 |

### 模型

| Method | Path | 说明 |
|---|---|---|
| `GET` | `/{provider}/v1/models` | 列出模型 |
| `GET` | `/{provider}/v1/models/{*model_id}` | 获取模型详情 |

### Gemini 原生

| Method | Path | 说明 |
|---|---|---|
| `GET` | `/{provider}/v1beta/models` | Gemini 模型列表 |
| `POST` | `/{provider}/v1beta/models/{*target}` | Gemini generateContent、streamGenerateContent、countTokens、embedContent |
| `POST` | `/{provider}/v1beta/{*target}` | Gemini v1beta 通配 |

### 文件

| Method | Path | 说明 |
|---|---|---|
| `POST` | `/{provider}/v1/files` | 上传文件 |
| `GET` | `/{provider}/v1/files` | 列出文件 |
| `GET` | `/{provider}/v1/files/{file_id}` | 获取文件元数据 |
| `DELETE` | `/{provider}/v1/files/{file_id}` | 删除文件 |
| `GET` | `/{provider}/v1/files/{file_id}/content` | 下载文件内容 |

### OAuth 和用量（仅管理员）

| Method | Path | 说明 |
|---|---|---|
| `GET` | `/{provider}/v1/oauth` | 发起 OAuth 授权流程 |
| `GET` | `/{provider}/v1/oauth/callback` | OAuth 回调 |
| `GET` | `/{provider}/v1/usage` | 查询上游 provider 用量/配额 |

## Provider Unscoped 路由（`/v1/*`、`/v1beta/*`）

与 scoped 路由端点相同，但不含 `{provider}` 路径前缀。Provider 从请求体的 model 字段解析，该字段必须包含 provider 前缀（如 `openai/gpt-4.1`），或匹配已配置的 model 别名。

### 推理

| Method | Path | 说明 |
|---|---|---|
| `POST` | `/v1/messages` | Claude Messages（从 model 前缀解析 provider） |
| `POST` | `/v1/messages/count_tokens` | Claude token 计数 |
| `POST` | `/v1/chat/completions` | OpenAI Chat Completions |
| `POST` | `/v1/responses` | OpenAI Responses |
| `POST` | `/v1/responses/input_tokens` | OpenAI Responses 输入 token 计数 |
| `POST` | `/v1/responses/compact` | OpenAI Responses compact 模式 |
| `POST` | `/v1/embeddings` | Embeddings |
| `POST` | `/v1/images/generations` | 图像生成 |
| `POST` | `/v1/images/edits` | 图像编辑 |

### 模型

| Method | Path | 说明 |
|---|---|---|
| `GET` | `/v1/models` | 列出模型（所有 provider） |
| `GET` | `/v1/models/{*model_id}` | 获取模型详情 |

### Gemini 原生

| Method | Path | 说明 |
|---|---|---|
| `GET` | `/v1beta/models` | Gemini 模型列表 |
| `POST` | `/v1beta/{*target}` | Gemini v1beta 通配 |

### 文件（unscoped）

Unscoped 文件路由从 `X-Provider` 请求头（而非 model 字段）解析 provider。

| Method | Path | 说明 |
|---|---|---|
| `POST` | `/v1/files` | 上传文件 |
| `GET` | `/v1/files` | 列出文件 |
| `GET` | `/v1/files/{file_id}` | 获取文件元数据 |
| `DELETE` | `/v1/files/{file_id}` | 删除文件 |
| `GET` | `/v1/files/{file_id}/content` | 下载文件内容 |

## WebSocket 路由

| Method | Path | 说明 |
|---|---|---|
| `GET` | `/{provider}/v1/responses` | OpenAI Responses WebSocket 流式传输 |
| `GET` | `/{provider}/v1beta/models/{*target}` | Gemini Live API |
| `GET` | `/v1/responses` | OpenAI Responses WebSocket（unscoped，从 model 前缀解析 provider） |

WebSocket 连接使用与 HTTP provider 路由相同的鉴权方式（请求头或查询参数中的 API key）。

## 请求示例

### Scoped 请求（provider 在 URL 中）

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

### Unscoped 请求（provider 在 model 字段中）

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

Provider 前缀（`openai/`）在请求发往上游前会被剥离。

### Gemini 原生（查询参数鉴权）

```bash
curl -sS "http://127.0.0.1:8787/aistudio/v1beta/models/gemini-2.5-flash:generateContent?key=<your-api-key>" \
  -H "Content-Type: application/json" \
  -d '{
    "contents": [{"parts": [{"text": "hello"}]}]
  }'
```
