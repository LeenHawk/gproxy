---
title: 第一条请求
description: 通过 gproxy v2 发送 OpenAI、Claude 和 Gemini 兼容请求。
---

gproxy v2 暴露 OpenAI、Anthropic 和 Gemini 兼容 HTTP surface。调用方使用 user API
key 鉴权；provider、route、route member、alias、rule、permission、quota 和
credential 决定请求转发到哪里。

写 `model` 字段前，先区分两种路由模式。

## 聚合路由

聚合入口位于 `/v1/*` 和 `/v1beta/*`。这种模式下，请求中的模型名会通过 v2 route
数据解析：

```text
client model -> alias 或 route name -> route member -> provider credential
```

快速开始 bundle 中的 route 名是 `main`：

```bash
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer sk-dev-local" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "main",
    "messages": [
      { "role": "user", "content": "Say hello." }
    ]
  }'
```

聚合模型列表由 gproxy 自己的 snapshot 返回：

```bash
curl http://127.0.0.1:8787/v1/models \
  -H "Authorization: Bearer sk-dev-local"
```

列表包含这个 key 有权限看到的 route 和 alias 名，不是所有上游 provider model 的原始转储。

## Scoped Provider 路由

Scoped 路由位于 `/{provider}/v1/*` 和 `/{provider}/v1beta/*`。这种模式下，URL 里的
provider 决定上游，`model` 是上游模型 id：

```bash
curl http://127.0.0.1:8787/openai-main/v1/chat/completions \
  -H "Authorization: Bearer sk-dev-local" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4.1-mini",
    "messages": [
      { "role": "user", "content": "Say hello." }
    ]
  }'
```

Scoped 模式绕过 route balancing，但仍会校验 user key、按 provider 名检查权限、执行
provider rules、选择 credential、进行兼容请求体改写、按实例设置记录日志并结算 usage。

## Claude Messages

Claude-compatible 调用使用 Anthropic message shape。聚合请求使用 route 或 alias 名：

```bash
curl http://127.0.0.1:8787/v1/messages \
  -H "x-api-key: sk-dev-local" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "main",
    "max_tokens": 256,
    "messages": [
      { "role": "user", "content": "Hello from Claude format." }
    ]
  }'
```

Scoped 模式把 provider 放到路径中，并使用上游模型 id：

```bash
curl http://127.0.0.1:8787/claude-main/v1/messages \
  -H "x-api-key: sk-dev-local" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4-5",
    "max_tokens": 256,
    "messages": [
      { "role": "user", "content": "Hello from Claude format." }
    ]
  }'
```

模型 id 必须存在于上游，或被该 provider 接受。v2 对 scoped model validation 保持宽松，
由 provider 返回真实结果。

## Gemini generateContent

Gemini 把模型放在 path 中。聚合请求在 `models/...` 段中使用 route 或 alias：

```bash
curl "http://127.0.0.1:8787/v1beta/models/main:generateContent" \
  -H "x-goog-api-key: sk-dev-local" \
  -H "Content-Type: application/json" \
  -d '{
    "contents": [
      { "parts": [ { "text": "Hello from Gemini format." } ] }
    ]
  }'
```

Scoped 请求使用 provider path 前缀。下面的例子假设你已经创建了一个名为
`aistudio-main` 的 Gemini-capable provider：

```bash
curl "http://127.0.0.1:8787/aistudio-main/v1beta/models/gemini-1.5-flash:generateContent" \
  -H "x-goog-api-key: sk-dev-local" \
  -H "Content-Type: application/json" \
  -d '{
    "contents": [
      { "parts": [ { "text": "Hello from Gemini format." } ] }
    ]
  }'
```

## 常见错误

| 现象 | 含义 |
| --- | --- |
| `401` | user API key 缺失、未知或已禁用。 |
| `403` | key 有效，但没有 route 或 provider 权限。 |
| `404` 或 unknown route | 聚合模式无法解析请求的 route 或 alias。 |
| `429` | rate limit 超限。 |
| `402` | quota precheck 失败。 |
| `413` | 请求体超过 native/edge 共用大小限制。 |

每个 gateway 响应都会带 `x-gproxy-request-id`，便于和日志关联。
