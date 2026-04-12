---
title: 发送第一个请求
description: 使用 OpenAI、Claude 或 Gemini 兼容接口通过 gproxy 发出你的第一个 LLM 请求。
---

gproxy 在标准的 OpenAI / Anthropic / Gemini HTTP 接口形状上接收流量。任何客户端
只要指向 gproxy 的 base URL，使用**用户** API key 鉴权即可。

下面的例子假设：

- gproxy 监听在 `http://127.0.0.1:8787`
- 用户 `alice` 的 API key 为 `sk-user-alice-1`
- 供应商 `openai-main` 暴露了 `gpt-4.1-mini` (及别名 `chat-default`)

## OpenAI 兼容: Chat Completions

```bash
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer sk-user-alice-1" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4.1-mini",
    "messages": [
      { "role": "user", "content": "用一句话打个招呼。" }
    ]
  }'
```

用别名替代真实模型 ID：

```json
{ "model": "chat-default", "messages": [ … ] }
```

当使用别名时，非流式响应的 `"model"` 字段会被改写回客户端发送的别名，
流式响应也会在引擎里逐 chunk 改写 —— 客户端从头到尾看到的是一致的名字。

## Anthropic 兼容: Messages

```bash
curl http://127.0.0.1:8787/v1/messages \
  -H "x-api-key: sk-user-alice-1" \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-5-sonnet-latest",
    "max_tokens": 256,
    "messages": [
      { "role": "user", "content": "你好" }
    ]
  }'
```

请求会被路由到用户有权限的任意 Anthropic 兼容供应商和模型。如果上游协议不同，
gproxy 会通过协议 `transform` 层完成翻译。

## Gemini 兼容: generateContent

```bash
curl "http://127.0.0.1:8787/v1beta/models/gemini-1.5-flash:generateContent" \
  -H "x-goog-api-key: sk-user-alice-1" \
  -H "Content-Type: application/json" \
  -d '{
    "contents": [
      { "parts": [ { "text": "你好" } ] }
    ]
  }'
```

## 列出模型

三种协议都有模型列表接口：

```bash
curl http://127.0.0.1:8787/v1/models \
  -H "Authorization: Bearer sk-user-alice-1"
```

返回值同时包含真实模型和别名，并按请求用户的权限过滤。
`GET /v1/models/{id}` 可单独查询 (别名也能查)。

## 会记录什么

当 `enable_usage = true` 时 (见 [TOML 配置参考](/zh-cn/reference/toml-config/))，
gproxy 会把每一次完成的请求的用量 (token、成本、用户、供应商、模型) 通过 `UsageSink`
worker 异步记录。你可以在控制台看到，或通过管理 API 查询。

若启用了 `enable_upstream_log` / `enable_downstream_log`，还会记录请求/响应的信封；
body 捕获有独立开关 —— 生产环境建议默认关闭。

## 常见错误排查

- **`401 unauthorized`** —— API key 缺失、不存在或已禁用。
- **`403 forbidden: model`** —— 该用户没有匹配目标模型的权限。检查
  `[[permissions]]` 或控制台的*权限*标签。
- **`429 rate_limited`** —— 命中了用户/模型限流。详见
  [权限、限流与配额](/zh-cn/guides/permissions/)。
- **`402 quota_exceeded`** —— 用户配额已用完。到控制台或管理 API 续额。
