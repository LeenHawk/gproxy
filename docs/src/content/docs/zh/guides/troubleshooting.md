---
title: 常见问题排查
description: GPROXY 常见错误、原因和解决方法。
---

## 1) `401 unauthorized`

**错误消息：**

- `missing API key`
- `invalid or disabled API key`
- `session expired or invalid`
- `user not found`
- `invalid username or password`
- `session token required (use /login to obtain one)`

**原因：**

- 请求没有携带鉴权头。需添加 `Authorization: Bearer <key>`、`x-api-key`、`x-goog-api-key` 或 `?key=<value>`。
- API key 不存在于数据库中，或所属用户已被禁用。
- Session token 已过期（session 仅存于内存，默认 24 小时有效，重启后失效）。
- 在 `/user/*` 路由上使用了 API key，这些路由要求通过 `/login` 获取 session token。

**解决方法：** 在管理控制台确认 key 存在且已启用。对于 session 路由，先调用 `POST /login`。

## 2) `403 forbidden`

**错误消息：**

- `admin access required`
- `user is disabled`

**原因：**

- 使用非管理员用户的 API key 调用 `/admin/*`。Admin 路由要求 key 所属用户 `is_admin = true`。
- 使用非管理员用户的 session token 调用 `/admin/*`。
- 用户账号在 session 创建后被禁用。Session 仍然有效，但用户检查失败。

**解决方法：** 使用属于管理员用户的 key 或 session。如果用户被意外禁用，重新启用即可。

## 3) `503 all eligible credentials exhausted`

**原因：**

- Provider 没有配置任何 credential。
- 所有 credential 被标记为 dead（上游返回持续性鉴权失败，如 401/403）。
- 所有 credential 处于冷却期（临时状态，在上游返回 429 或 5xx 后触发）。
- 目标模型有访问限制，没有 credential 有权访问。

**解决方法：**

1. 在管理控制台检查 credential 状态（`/admin/credential-statuses/query`）。
2. 如果 credential 为 dead，验证对应的 API key 在上游是否仍然有效。
3. 如果 credential 处于冷却期，等待冷却时间到期，或通过 `/admin/credential-statuses/update` 手动重置状态。
4. 增加更多 credential 以分散负载。

## 4) `model must have provider prefix (provider/model) or match an alias`

**出现场景：** 调用 unscoped 路由（`/v1/chat/completions`、`/v1/messages` 等），URL 中没有 provider。

**原因：** 请求体中的 `model` 字段既没有 provider 前缀，也不匹配任何已配置的 model 别名。

**解决方法：** 三选一：

- 使用 `"model": "openai/gpt-4.1"`（带 provider 前缀）。
- 使用 scoped 路由：`/{provider}/v1/chat/completions`，model 字段只需写 `"gpt-4.1"`。
- 在管理控制台配置 model 别名，将别名映射到 provider + model。

## 5) `unsupported <channel> request route: (<operation>, <protocol>)`

**示例：** `unsupported openai request route: (Embedding, ClaudeMessages)`

**原因：** Provider 的 dispatch 表没有对应 (operation, protocol) 组合的路由。通常是向目标 channel 不支持的协议端点发送了请求。

**解决方法：**

- 检查 provider 支持哪些 operation 和 protocol。使用 `/admin/providers/default-dispatch` 查看 channel 的 dispatch 表。
- 使用正确的端点。例如，不要向纯 OpenAI provider 发送 Claude Messages 协议请求。
- 如果该 provider 应当支持此组合，更新 provider 的 dispatch 配置。

## 6) `no request transform for (<src_op>, <src_proto>) -> (<dst_op>, <dst_proto>)`

**原因：** GPROXY 收到某一协议（如 OpenAI Chat）的请求，dispatch 表将其路由到另一协议（如 Gemini），但该协议对之间没有实现跨协议转换。

**解决方法：**

- 尽量使用同协议透传（向 OpenAI 兼容 channel 发送 OpenAI 请求）。
- 查看 provider 的 dispatch 表，确认配置了哪些 transform。
- 并非所有跨协议转换都存在，某些组合被有意标记为不支持。

## 7) `Failed to deserialize the JSON body`

**原因：** 请求体缺少必填字段、字段类型错误或 JSON 格式不合法。

**解决方法：**

- 检查请求体是否符合对应端点的 API schema。
- 常见错误：缺少 `model` 字段、缺少 `messages` 数组、`content` 类型不正确。
- 验证 JSON 语法（末尾逗号、未加引号的 key 等）。
