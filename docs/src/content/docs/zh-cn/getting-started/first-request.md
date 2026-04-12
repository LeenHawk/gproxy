---
title: 发送第一个请求
description: 使用 OpenAI、Claude 或 Gemini 兼容接口通过 GPROXY 发出你的第一个 LLM 请求。
---

GPROXY 在标准的 OpenAI / Anthropic / Gemini HTTP 接口形状上接收流量。任何客户端
只要指向 GPROXY 的 base URL，使用**用户** API key 鉴权即可。

不过在第一次发请求之前，先要理解 **GPROXY 是怎么选上游供应商的** —— 它有两种
不同的路由模式，会直接影响你 `model` 字段该怎么写。

## 两种路由模式

### 聚合入口 —— `/v1/...`、`/v1beta/...`

一个 base URL 扇出到所有供应商。供应商名必须**编码进 model 字段**，
形如 `provider/model`（或者命中别名）：

```text
POST /v1/chat/completions
{ "model": "openai-main/gpt-4.1-mini", ... }
```

- `openai-main` 是你配置里供应商的名字。
- `gpt-4.1-mini` 是上游模型 id。
- Gemini 风格的 URI 路径是一样的规则：
  `/v1beta/models/openai-main/gpt-4.1-mini:generateContent`。
- 如果 `model` 命中了**别名**，就不需要前缀了 —— 别名本身已经解析到
  具体的 `(provider, model)` 对。

### 限定作用域入口 —— `/{provider}/v1/...`

供应商名直接在 URL 路径里，`model` 字段就写上游的原始 id，不带前缀：

```text
POST /openai-main/v1/chat/completions
{ "model": "gpt-4.1-mini", ... }
```

这种写法最接近"把 GPROXY 当成上游直接调用"，也最适合那种
把 base URL 和模型名硬编码在一起的客户端。

:::tip
两种形式随客户端挑，**同一个 GPROXY 实例同时提供**两种入口 —— 对每种协议
（OpenAI / Claude / Gemini）都成立。
:::

## 示例

下面的例子假设：

- GPROXY 监听在 `http://127.0.0.1:8787`
- 管理员 key `sk-admin-1`（来自 [快速开始](/zh-cn/getting-started/quick-start/)）
- 供应商 `openai-main` 暴露了 `gpt-4.1-mini`

### OpenAI Chat Completions

聚合 `/v1`：

```bash
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer sk-admin-1" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "openai-main/gpt-4.1-mini",
    "messages": [
      { "role": "user", "content": "用一句话打个招呼。" }
    ]
  }'
```

限定作用域 `/openai-main/v1`：

```bash
curl http://127.0.0.1:8787/openai-main/v1/chat/completions \
  -H "Authorization: Bearer sk-admin-1" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4.1-mini",
    "messages": [
      { "role": "user", "content": "用一句话打个招呼。" }
    ]
  }'
```

如果你定义过别名（例如 `chat-default`），两种入口下都可以直接用
别名、不加前缀 —— 别名内部已经解析到具体的 `(provider, model)`：

```json
{ "model": "chat-default", "messages": [ ... ] }
```

非流式响应的 `"model"` 字段会被改写回客户端发送的别名；流式 chunk 也
在引擎里逐 chunk 改写。

### Anthropic Messages

```bash
curl http://127.0.0.1:8787/v1/messages \
  -H "x-api-key: sk-admin-1" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "anthropic-main/claude-3-5-sonnet-latest",
    "max_tokens": 256,
    "messages": [ { "role": "user", "content": "你好" } ]
  }'
```

或者使用限定作用域形式：

```bash
curl http://127.0.0.1:8787/anthropic-main/v1/messages \
  -H "x-api-key: sk-admin-1" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-5-sonnet-latest",
    "max_tokens": 256,
    "messages": [ { "role": "user", "content": "你好" } ]
  }'
```

### Gemini generateContent

聚合 —— 供应商前缀嵌入到 `models/...` 这一段：

```bash
curl "http://127.0.0.1:8787/v1beta/models/vertex-main/gemini-1.5-flash:generateContent" \
  -H "x-goog-api-key: sk-admin-1" \
  -H "Content-Type: application/json" \
  -d '{ "contents": [ { "parts": [ { "text": "你好" } ] } ] }'
```

限定作用域：

```bash
curl "http://127.0.0.1:8787/vertex-main/v1beta/models/gemini-1.5-flash:generateContent" \
  -H "x-goog-api-key: sk-admin-1" \
  -H "Content-Type: application/json" \
  -d '{ "contents": [ { "parts": [ { "text": "你好" } ] } ] }'
```

## 列出模型

两种入口都提供模型列表：

```bash
# 聚合 —— 返回用户可见的所有供应商的模型。
curl http://127.0.0.1:8787/v1/models \
  -H "Authorization: Bearer sk-admin-1"

# 限定作用域 —— 只返回 openai-main 上的模型。
curl http://127.0.0.1:8787/openai-main/v1/models \
  -H "Authorization: Bearer sk-admin-1"
```

别名和真实模型一起作为一等条目返回，并按请求用户的权限过滤。
`GET /v1/models/{id}` 可单独查询（别名也能查）。

## 会记录什么

当 `enable_usage = true` 时（见 [TOML 配置参考](/zh-cn/reference/toml-config/)），
GPROXY 把每一次完成的请求的用量（token、成本、用户、供应商、模型）通过
`UsageSink` worker 异步记录。控制台和管理 API 都能查询。

若启用了 `enable_upstream_log` / `enable_downstream_log`，还会记录
请求/响应的信封；body 捕获有独立开关 —— 生产环境建议默认关闭。

## 常见错误排查

- **`401 unauthorized`** —— API key 缺失、不存在或已禁用。
- **`400` / `provider prefix`** —— 你打的是 `/v1`，但 `model` 既没有 `provider/`
  前缀也不匹配任何别名。要么加前缀，要么改用限定作用域入口，要么给这个名字
  定义一个别名。
- **`403 forbidden: model`** —— 该用户没有匹配目标模型的权限。检查
  `[[permissions]]` 或控制台的*权限*标签。
- **`429 rate_limited`** —— 命中了用户/模型限流。详见
  [权限、限流与配额](/zh-cn/guides/permissions/)。
- **`402 quota_exceeded`** —— 用户配额已用完。到控制台或管理 API 续额。
