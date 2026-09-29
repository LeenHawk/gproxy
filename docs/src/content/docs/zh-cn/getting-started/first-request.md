---
title: "发送第一个请求"
description: "使用 OpenAI、Claude 和 Gemini API 调用网关，启用流式响应并排查常见错误。"
---

完成[快速开始](/zh-cn/getting-started/quick-start/)后，应该已有名为 `fast` 的模型路由和一把网关 API Key。下面的例子使用默认地址；修改过端口时请一起调整。

```sh
export GPROXY_KEY='your-gproxy-api-key'
```

网关接受 `Authorization: Bearer <key>`、`x-api-key` 和 `x-goog-api-key`。请使用网关签发的密钥，不要填上游凭证。

## OpenAI Chat Completions

```sh
curl -sS http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"fast","messages":[{"role":"user","content":"Say hello."}]}'
```

## OpenAI Responses

```sh
curl -sS http://127.0.0.1:8787/v1/responses \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"fast","input":"Say hello."}'
```

## Claude Messages

```sh
curl -sS http://127.0.0.1:8787/v1/messages \
  -H "x-api-key: $GPROXY_KEY" \
  -H 'anthropic-version: 2023-06-01' \
  -H 'content-type: application/json' \
  -d '{"model":"fast","max_tokens":256,
       "messages":[{"role":"user","content":"Say hello."}]}'
```

## Gemini GenerateContent

```sh
curl -sS "http://127.0.0.1:8787/v1beta/models/fast:generateContent" \
  -H "x-goog-api-key: $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"contents":[{"parts":[{"text":"Say hello."}]}]}'
```

## 流式响应

```sh
curl -sSN http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"fast","stream":true,
       "messages":[{"role":"user","content":"Count to three."}]}'
```

## Gemini 流式响应

```sh
curl -sSN "http://127.0.0.1:8787/v1beta/models/fast:streamGenerateContent?alt=sse" \
  -H "x-goog-api-key: $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"contents":[{"parts":[{"text":"Count to three."}]}]}'
```

返回格式与客户端请求的协议一致。能否调用成功取决于路由成员、上游能力与转换支持；某个上游不能执行的操作，不会因为更换接口路径而自动获得支持。

## 模型列表

控制台中的模型目录是配置网关模型名的入口。通过 API 查询调用方可见的网关目录：

```sh
curl -sS http://127.0.0.1:8787/portal/api/models \
  -H "Authorization: Bearer $GPROXY_KEY"
```

目录中的 `permitted` 表示当前调用方是否有权限。`GET /v1/models` 转发的是所选上游的模型列表，与网关目录不同。Application 不在 HTTP 端口开放 `/portal/api`，请在应用内查看目录。

## 指定供应商或命名空间

| 地址 | 请求中的模型名 | 目标 |
| --- | --- | --- |
| `/v1/chat/completions` | `fast` | 名为 `fast` 的路由 |
| `/openai-main/v1/chat/completions` | 上游模型 ID | `openai-main` 供应商 |
| `/acme/v1/chat/completions` | `fast` | 名为 `acme/fast` 的路由 |

命名规则见[模型与路由](/zh-cn/guides/models/)。

## 常见错误

| 状态码 | 含义 |
| --- | --- |
| `400` | 参数格式或配置值无效 |
| `401` | 密钥缺失、无效、停用或过期 |
| `403` | 权限或 OAuth 授权范围不足 |
| `404` | 模型、路由或资源不存在 |
| `429` | 请求受到限流或限额约束 |
| `5xx` | 网关或上游调用失败，需结合日志检查 |

请求完成后可在控制台查看用量与请求记录。正文是否保存取决于日志设置，详见[用量、日志与审计](/zh-cn/guides/observability/)。
