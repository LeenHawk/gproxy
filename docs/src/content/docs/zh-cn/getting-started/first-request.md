---
title: 发送第一个请求
description: 通过 GPROXY 发送 OpenAI Chat、OpenAI Responses、Claude Messages 和 Gemini 请求，使用流式、列出模型、使用命名前缀，并在事后找到这次请求。
---

GPROXY 在每种接受的线格式的原生路径上应答。用户 API 密钥用于鉴权；公开模型名选择
负载均衡，它的成员、权限、配额、规则和凭证决定请求去向。下面的示例假设已经按
[快速开始](/zh-cn/getting-started/quick-start/)创建了公开模型名 `main` 和密钥
`sk-<your-key>`。

## 鉴权

在任意路径上，用以下任一请求头发送密钥：

```text
Authorization: Bearer sk-<your-key>
x-api-key: sk-<your-key>
x-goog-api-key: sk-<your-key>
```

密钥必须已启用且未过期，并且对请求解析到的 Provider 拥有允许权限。

## OpenAI Chat Completions

```bash
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer sk-<your-key>" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "main",
    "messages": [
      { "role": "user", "content": "Say hello." }
    ]
  }'
```

## OpenAI Responses

```bash
curl http://127.0.0.1:8787/v1/responses \
  -H "Authorization: Bearer sk-<your-key>" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "main",
    "input": "Say hello."
  }'
```

## Claude Messages

```bash
curl http://127.0.0.1:8787/v1/messages \
  -H "x-api-key: sk-<your-key>" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "main",
    "max_tokens": 256,
    "messages": [
      { "role": "user", "content": "Say hello." }
    ]
  }'
```

## Gemini GenerateContent

Gemini 把模型放在路径中：

```bash
curl "http://127.0.0.1:8787/v1beta/models/main:generateContent" \
  -H "x-goog-api-key: sk-<your-key>" \
  -H "Content-Type: application/json" \
  -d '{
    "contents": [
      { "parts": [ { "text": "Say hello." } ] }
    ]
  }'
```

如果负载均衡的成员使用另一种格式，GPROXY 在发出时转换请求、在返回时转换响应。客
户端在任何情况下看到的都是自己的格式。

## 流式

对 OpenAI Chat、OpenAI Responses 和 Claude Messages，在请求体中加入
`"stream": true`；响应是该格式自身事件形状的 server-sent events：

```bash
curl -N http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer sk-<your-key>" \
  -H "Content-Type: application/json" \
  -d '{ "model": "main", "stream": true,
        "messages": [ { "role": "user", "content": "Count to five." } ] }'
```

对 Gemini，调用 `:streamGenerateContent`。不带查询参数时，流是 Gemini 的增量 JSON
数组；`?alt=sse` 选择 server-sent events：

```bash
curl -N "http://127.0.0.1:8787/v1beta/models/main:streamGenerateContent?alt=sse" \
  -H "x-goog-api-key: sk-<your-key>" \
  -H "Content-Type: application/json" \
  -d '{ "contents": [ { "parts": [ { "text": "Count to five." } ] } ] }'
```

OpenAI Responses 也支持 WebSocket：向 `GET /v1/responses` 发起升级请求会打开一个
会话，它经过与 HTTP 调用相同的准入和结算。

## 列出模型

```bash
curl http://127.0.0.1:8787/v1/models \
  -H "Authorization: Bearer sk-<your-key>"
```

`GET /v1/models` 以 OpenAI 或 Claude 的形状应答，`GET /v1beta/models` 以 Gemini 的
形状应答，`GET /v1/models/{id}` 返回单条记录。列表包含该密钥可以使用的公开模型名
和变体。开启了**从上游刷新模型列表**设置（默认开启）的 Provider 会被并发询问其目
录。列表由网关自身应答，仍然经过准入，并记录一条零成本结算。

## 命名前缀

把目标名放在第一段路径中即可直接选择它，其余部分是原生路径：

```bash
curl http://127.0.0.1:8787/openai-main/v1/chat/completions \
  -H "Authorization: Bearer sk-<your-key>" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4.1-mini",
    "messages": [ { "role": "user", "content": "Say hello." } ]
  }'
```

| 第一段路径 | 对 `model` 的影响 |
| --- | --- |
| Provider 名，例如 `openai-main` | `model` 是上游模型 id。直接使用该 Provider 的凭证池，跳过负载均衡选择。 |
| 路由名 | 像聚合模式一样使用该路由的成员。 |
| 命名空间，例如公开模型 `openai/gpt-4.1` 的 `openai` | `model` 是斜杠之后的部分。 |

命名请求仍然校验密钥、针对所选 Provider 检查权限、应用它的规则集、选择凭证并结算
用量。第一段路径与任何目标都不匹配时，按聚合路径的一部分处理。Codex CLI 使用的就
是这种形式：门户的连接片段把它指向 `/codex/backend-api/...` 和 `/codex/oauth/...`。

## 错误

| 状态码 | 含义 |
| --- | --- |
| `400` | 路径或操作不受支持，或请求体无效。 |
| `401` | 密钥缺失、未知、已停用或已过期。 |
| `402` | 成本配额已用尽。 |
| `403` | 密钥对解析到的 Provider 没有允许权限，或有拒绝规则生效。 |
| `404` | 公开模型名、路由或 Provider 不存在。 |
| `413` | 请求体超过 100 MiB 上限。 |
| `429` | 触发限流；响应体中有 `retry_after_secs`。 |
| `502` | 没有可用凭证，或所有上游尝试都失败。 |

错误体使用 OpenAI 的信封格式：`{"error":{"message":"..."}}`。

## 事后找到这次请求

每个响应都带有形如 `<instance-id>-<random>-<sequence>` 的 `x-request-id` 头。在控
制台中：

- **统计 → 用量**显示请求数、输入与输出 token、缓存读写和结算成本，可按 Provider、
  凭证、用户、密钥或模型筛选。
- **统计 → 请求审计**列出每个客户端请求及其产生的全部上游调用，可按用户、密钥、
  Provider、状态或请求 ID 筛选。只有在**设置**中开启了对应的捕获开关时才会显示请求
  头和请求体，且除非在那里关闭脱敏，否则都经过脱敏。
- **统计 → 管理操作**记录控制台变更和通道登录。

登录 `/portal` 的用户可以看到自己的用量；当运维者在**设置 → 用户门户**中开启了
**显示最近结算请求**时，还会看到**最近结算请求**表，包含 Provider、操作、上游模
型、token、成本和延迟——绝不包含请求或响应体。
