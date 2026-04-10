---
title: 自定义 Channel
description: 接入兼容 OpenAI、Claude 或 Gemini 协议的上游服务，无需修改代码。
---

自定义 Channel 可以将流量代理到任何使用标准 LLM 协议的上游服务。无需改代码，无需重新编译——只要配置即可。

## 支持的上游协议格式

Custom 模式适用于兼容以下协议的上游：

- **OpenAI** -- `/v1/chat/completions`、`/v1/responses`、`/v1/models`、`/v1/embeddings`、`/v1/images/generations`
- **Claude** -- `/v1/messages`、`/v1/messages/count_tokens`、`/v1/models`
- **Gemini** -- `/v1beta/models/{model}:generateContent`、`/v1beta/models/{model}:streamGenerateContent`、`/v1beta/models/{model}:countTokens`、`/v1beta/models/{model}:embedContent`

如果上游使用非标准签名、自定义鉴权握手或深度修改过的请求/响应结构，custom 模式无法满足需求，需要实现原生 Channel。

## 最小配置

```toml
[[providers]]
name = "my-upstream"
channel = "custom"
settings = { base_url = "https://api.example.com" }
credentials = [{ api_key = "sk-replace-me" }]
```

## 配置项

| 字段 | 默认值 | 说明 |
|------|--------|------|
| `base_url` | （必填） | 上游 base URL |
| `user_agent` | （无） | 自定义 `User-Agent` 请求头 |
| `max_retries_on_429` | `3` | 收到 429 限流响应时的重试次数 |
| `auth_scheme` | `bearer` | 鉴权方式：`bearer`、`x-api-key` 或 `query-key` |

鉴权方式说明：

- `bearer` -- 发送 `Authorization: Bearer <api_key>` 请求头
- `x-api-key` -- 发送 `x-api-key: <api_key>` 请求头
- `query-key` -- 在 URL 后追加 `?key=<api_key>`

## 用 `mask_table` 剥离请求字段

转发到上游前，从请求体中删除指定字段。当上游拒绝未知字段时非常有用。

```toml
[[providers]]
name = "my-upstream"
channel = "custom"

[providers.settings]
base_url = "https://api.example.com"

[providers.settings.mask_table]
rules = [
  { method = "POST", path = "/v1/chat/completions", remove_fields = ["metadata"] },
  { method = "POST", path = "/v1/responses", remove_fields = ["metadata", "previous_response_id"] },
]
```

`mask_table` 能做的事：

- 按 HTTP 方法和路径匹配请求。
- 删除请求体中的顶层 JSON 字段。

`mask_table` 不能做的事：

- 改写响应体。
- 注入自定义签名或鉴权逻辑。
- 实现任意协议转换。

## 自定义 dispatch 规则

默认情况下，custom channel 会为所有 operation/protocol 组合注册通用透传。你可以通过显式 dispatch 规则来限制或重定向特定操作。

```toml
[providers.dispatch]
rules = [
  { route = { operation = "ModelList", protocol = "OpenAi" }, implementation = "Passthrough" },
  { route = { operation = "ModelGet", protocol = "OpenAi" }, implementation = "Passthrough" },
  { route = { operation = "GenerateContent", protocol = "OpenAiChatCompletion" }, implementation = "Passthrough" },
  { route = { operation = "StreamGenerateContent", protocol = "OpenAiChatCompletion" }, implementation = "Passthrough" },
  { route = { operation = "CountToken", protocol = "OpenAi" }, implementation = "Local" },
]
```

Dispatch 实现方式：

| Implementation | 行为 |
|----------------|------|
| `Passthrough` | 按原协议直接转发到上游 |
| `TransformTo` | 转换为另一种 operation/protocol 组合后再发送 |
| `Local` | 本地处理，不请求上游 |
| `Unsupported` | 返回 501 |

## 能力边界

Custom channel 可以选择 dispatch 路由，但仅限于 GPROXY 已有的 operation 和 protocol 模型。你可以选择哪些操作走 passthrough、transform、local 或 unsupported，但无法引入新的协议或实现自定义转换逻辑。

可用 operation：`ModelList`、`ModelGet`、`CountToken`、`GenerateContent`、`StreamGenerateContent`、`Embedding`、`CreateImage`、`StreamCreateImage`、`CreateImageEdit`、`StreamCreateImageEdit`、`Compact`、`OpenAiResponseWebSocket`、`GeminiLive`。

可用 protocol：`OpenAi`、`OpenAiResponse`、`OpenAiChatCompletion`、`Claude`、`Gemini`、`GeminiNDJson`。
