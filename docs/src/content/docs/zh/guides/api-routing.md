---
title: API 与路由
description: 完整的 METHOD + PATH 路由清单，以及协议路由规则。
---

## 错误格式

所有错误统一返回：

```json
{ "error": "..." }
```

## 鉴权头与凭证来源

管理端与用户端通常使用：

- `x-api-key`

Provider 路由还支持：

- `x-api-key`
- `x-goog-api-key`
- `Authorization: Bearer ...`
- Gemini 场景 query `?key=...`（服务会归一化为 `x-api-key`）

## 入口与静态资源路由

| Method | Path | 说明 |
|---|---|---|
| `GET` | `/` | 管理后台首页 |
| `GET` | `/assets/{*path}` | 管理后台静态资源 |
| `GET` | `/favicon.ico` | 返回 `204 No Content` |

## Provider Unscoped 路由

| Method | Path | 功能说明 |
|---|---|---|
| `POST` | `/v1/messages` | Claude 风格消息生成入口（统一入口） |
| `POST` | `/v1/messages/count_tokens` | Claude 风格 token 计数 |
| `POST` | `/v1/chat/completions` | OpenAI Chat Completions 入口 |
| `POST` | `/v1/responses` | OpenAI Responses 入口 |
| `GET` | `/v1/responses` | 响应升级检测入口（当前建议使用 HTTP `POST`） |
| `POST` | `/v1/responses/input_tokens` | OpenAI 输入 token 计数 |
| `POST` | `/v1/embeddings` | Embedding 入口 |
| `POST` | `/v1/responses/compact` | OpenAI Compact 入口 |
| `GET` | `/v1/models` | 模型列表查询 |
| `GET` | `/v1/models/{*model_id}` | 模型详情查询 |
| `GET` | `/v1beta/models` | Gemini 风格模型列表入口 |
| `GET` | `/v1beta/{*target}` | Gemini 风格 `models.get` 等 GET 目标入口 |
| `POST` | `/v1beta/{*target}` | Gemini 风格 `generateContent/countTokens/embedContent` 等入口 |
| `POST` | `/v1/{*target}` | Provider 自定义 `v1` 目标透传入口 |

规则：

- 调用 unscoped 时，`model` 需要 provider 前缀（例如 `openai/gpt-4.1`）。
- Gemini path 目标也需要可解析出 provider（例如 `models/aistudio/gemini-2.5-flash:generateContent`）。
- `GET /v1/responses` 走升级检测逻辑（当前 WebSocket 上游未实现，建议使用 `POST /v1/responses`）。

## Provider Scoped 路由

| Method | Path | 功能说明 |
|---|---|---|
| `GET` | `/{provider}/v1/oauth` | 发起 OAuth 授权流程 |
| `GET` | `/{provider}/v1/oauth/callback` | OAuth 回调处理 |
| `GET` | `/{provider}/v1/usage` | 查询上游 usage（支持渠道） |
| `GET` | `/{provider}/v1/realtime` | Realtime 升级入口 |
| `GET` | `/{provider}/v1/realtime/{*tail}` | 带 tail 的 Realtime 升级入口 |
| `POST` | `/{provider}/v1/messages` | Claude 风格消息生成 |
| `POST` | `/{provider}/v1/messages/count_tokens` | Claude 风格 token 计数 |
| `POST` | `/{provider}/v1/chat/completions` | OpenAI Chat Completions |
| `POST` | `/{provider}/v1/responses` | OpenAI Responses |
| `GET` | `/{provider}/v1/responses` | Responses 升级检测入口 |
| `POST` | `/{provider}/v1/responses/input_tokens` | OpenAI 输入 token 计数 |
| `POST` | `/{provider}/v1/embeddings` | Embedding |
| `POST` | `/{provider}/v1/responses/compact` | Compact 响应入口 |
| `GET` | `/{provider}/v1/models` | 模型列表 |
| `GET` | `/{provider}/v1/models/{*model_id}` | 模型详情 |
| `GET` | `/{provider}/v1beta/models` | Gemini 风格模型列表 |
| `GET` | `/{provider}/v1beta/{*target}` | Gemini 风格 GET 目标 |
| `POST` | `/{provider}/v1beta/{*target}` | Gemini 风格 POST 目标 |
| `POST` | `/{provider}/v1/{*target}` | Provider `v1` 目标透传 |

当前支持 OAuth 的内置渠道：

- `codex`
- `claudecode`
- `geminicli`
- `antigravity`

## 支持的 Gemini 方法

| Method | 路径示例 | 功能说明 |
|---|---|---|
| `GET` | `/v1beta/models` 或 `/{provider}/v1beta/models` | `models.list`，列出可用 Gemini 模型 |
| `GET` | `/v1beta/models/{model}`（经 `/{*target}`） | `models.get`，查询单模型详情 |
| `POST` | `/v1beta/models/{model}:countTokens` | `countTokens`，计算输入 token 数 |
| `POST` | `/v1beta/models/{model}:generateContent` | `generateContent`，非流式生成 |
| `POST` | `/v1beta/models/{model}:streamGenerateContent` | `streamGenerateContent`，流式生成（SSE/NDJSON） |
| `POST` | `/v1beta/models/{model}:embedContent` | `embedContent`，向量嵌入 |

## 管理接口

| Method | Path | 功能说明 |
|---|---|---|
| `GET` | `/admin/global-settings` | 读取全局配置 |
| `POST` | `/admin/global-settings/upsert` | 更新全局配置 |
| `POST` | `/admin/system/self_update` | 触发系统自更新 |
| `GET` | `/admin/config/export-toml` | 导出 TOML 配置 |
| `POST` | `/admin/config/import-toml` | 导入 TOML 配置 |
| `POST` | `/admin/providers/query` | 查询 provider |
| `POST` | `/admin/providers/upsert` | 新增/更新 provider |
| `POST` | `/admin/providers/delete` | 删除 provider |
| `POST` | `/admin/credentials/query` | 查询凭证 |
| `POST` | `/admin/credentials/upsert` | 新增/更新凭证 |
| `POST` | `/admin/credentials/delete` | 删除凭证 |
| `POST` | `/admin/credential-statuses/query` | 查询凭证健康状态 |
| `POST` | `/admin/credential-statuses/upsert` | 新增/更新凭证健康状态 |
| `POST` | `/admin/credential-statuses/delete` | 删除凭证健康状态 |
| `POST` | `/admin/users/query` | 查询用户 |
| `POST` | `/admin/users/upsert` | 新增/更新用户 |
| `POST` | `/admin/users/delete` | 删除用户 |
| `POST` | `/admin/user-keys/query` | 查询用户密钥 |
| `POST` | `/admin/user-keys/upsert` | 新增/更新用户密钥 |
| `POST` | `/admin/user-keys/delete` | 删除用户密钥 |
| `POST` | `/admin/requests/upstream/query` | 查询上游请求审计 |
| `POST` | `/admin/requests/downstream/query` | 查询下游请求审计 |
| `POST` | `/admin/usages/query` | 查询 usage 明细 |
| `POST` | `/admin/usages/summary` | 查询 usage 汇总 |

## 用户接口（`/user/*`，完整）

| Method | Path | 功能说明 |
|---|---|---|
| `POST` | `/user/keys/query` | 查询当前用户自己的 key |
| `POST` | `/user/keys/upsert` | 新增/更新当前用户自己的 key |
| `POST` | `/user/keys/delete` | 删除当前用户自己的 key |
| `POST` | `/user/usages/query` | 查询当前用户 usage 明细 |
| `POST` | `/user/usages/summary` | 查询当前用户 usage 汇总 |

用户 key 归一化规则：

- 存储为 `u{user_id}_<raw_key>`
- 如果输入已经带此前缀，则保持不变

## 调用示例

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
