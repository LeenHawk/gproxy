---
title: 发送第一个请求
description: 用 OpenAI、Claude 和 Gemini 的原生路径经 GPROXY 发请求，流式、三个挂载点，以及它回答的错误。
---

GPROXY 在每种被接受的线格式的原生路径上应答。网关 API key 认证调用方，`model` 名决定
请求去哪。下面的例子假设有一个名为 `fast` 的公开模型名，以及一把按
[快速开始](/zh-cn/getting-started/quick-start/)创建的 key。

```sh
export GPROXY_KEY='sk-…'
```

## 认证

在任意路径上，key 可以放在这三个 header 中的任意一个：

```text
Authorization: Bearer sk-…
x-api-key: sk-…
x-goog-api-key: sk-…
```

三者到达同一个身份。key 还会按"去掉前导 `sk-` 或 `at-` 之后的摘要"再查一次，因此同一份
密钥写成 `sk-X`、`at-X` 或裸 `X` 都是**同一把 key**：配额与用量跟着 key 走，而不是跟着
它被怎么敲出来走。

每一次认证失败都是 `401`，从不是 `403`——被禁用的 key、过期的 key、被撤销的授权和从未
存在过的 key，从外面看必须无法区分：

```json
{"error":{"code":"unauthorized","message":"unauthorized"}}
```

## OpenAI Chat Completions

```sh
curl -s http://127.0.0.1:7070/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"fast","messages":[{"role":"user","content":"Say hello."}]}'
```

## OpenAI Responses

```sh
curl -s http://127.0.0.1:7070/v1/responses \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"fast","input":"Say hello."}'
```

## Claude Messages

```sh
curl -s http://127.0.0.1:7070/v1/messages \
  -H "x-api-key: $GPROXY_KEY" \
  -H 'anthropic-version: 2023-06-01' \
  -H 'content-type: application/json' \
  -d '{"model":"fast","max_tokens":256,
       "messages":[{"role":"user","content":"Say hello."}]}'
```

当 `fast` 背后的成员是一个 OpenAI 形状的上游时，回来的仍然是 Claude Messages：

```json
{"type":"message","id":"chatcmpl-mock-1",
 "content":[{"type":"text","text":"Hello from the mock upstream."}],
 "model":"gpt-4o-mini","role":"assistant","stop_reason":"end_turn",
 "usage":{"input_tokens":11,"output_tokens":7}}
```

## Gemini GenerateContent

Gemini 把模型放在路径里：

```sh
curl -s "http://127.0.0.1:7070/v1beta/models/fast:generateContent" \
  -H "x-goog-api-key: $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"contents":[{"parts":[{"text":"Say hello."}]}]}'
```

```json
{"candidates":[{"content":{"parts":[{"text":"Hello from the mock upstream."}],
 "role":"model"},"finishReason":"STOP","index":0}],
 "usageMetadata":{"promptTokenCount":11,"totalTokenCount":18},
 "modelVersion":"gpt-4o-mini","responseId":"chatcmpl-mock-1"}
```

有几条路径（`/v1/models`、`/v1/models/{id}`、`/v1/files`…）被 OpenAI、Claude 和 Gemini
的 v1 surface 拼成了同一个样子。方言决定向转换器要哪一套线类型，猜错就是客户端解析不了
的 body。判别依据是**客户端自己的认证 header**——`x-goog-api-key` 是 Gemini 的、
`anthropic-version` 是 Claude 的——因为那是客户端主动提供的关于它自己的证据。没有证据
时第一行胜出，而表的顺序让那一行是 OpenAI。

## 流式

对三种靠 body 标志的方言，加上 `"stream": true`。响应是该格式自己的事件形状的 SSE：

```sh
curl -sN http://127.0.0.1:7070/v1/chat/completions \
  -H "Authorization: Bearer $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"model":"fast","stream":true,
       "messages":[{"role":"user","content":"Count to three."}]}'
```

由 OpenAI 上游承接的 `/v1/messages` 流式请求是**逐事件翻译**的，不是缓冲后重发：

```text
event: message_start
data: {"type":"message_start","message":{"type":"message","id":"chatcmpl-mock-1",…}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}
```

带 `stream: true` 的路径和不带的是**两个不同的操作**——不同的规则、不同的结算——所以这个
标志在入口就被读取，而不是留给渠道去发现。

Gemini 则把它写在路径里。不带 query 时流是 Gemini 的增量 JSON 数组；`?alt=sse` 选择 SSE：

```sh
curl -sN "http://127.0.0.1:7070/v1beta/models/fast:streamGenerateContent?alt=sse" \
  -H "x-goog-api-key: $GPROXY_KEY" \
  -H 'content-type: application/json' \
  -d '{"contents":[{"parts":[{"text":"Count to three."}]}]}'
```

无论上游产出什么，调用方拿到的都是它要的那种分帧。

## 列出模型

```sh
curl -s http://127.0.0.1:7070/v1/models -H "Authorization: Bearer $GPROXY_KEY"
```

v4 的 `GET /v1/models` 是**转发给某个 Provider** 的，回答的是那个上游自己的目录，不是
由你的配置合成出来的。*你*发布的那份名字清单在用户面，并且会标出这个调用方能不能调：

```sh
curl -s http://127.0.0.1:7070/portal/api/models -H "Authorization: Bearer $GPROXY_KEY"
```

```json
[{"name":"custom/gpt-4o-mini","providerCount":1,"channelIds":["custom"],"permitted":true},
 {"name":"fast","providerCount":1,"channelIds":["custom"],"permitted":true}]
```

这份清单什么都不省略。调用方规则触达不到的名字仍然在里面，只是 `permitted: false`——
因为一份会静默省略的清单会让"这个模型 404 了"和"你没有权限用这个模型"变成同一个观察。

## 三个挂载点

```sh
# 聚合——模型名决定一切
curl -s http://127.0.0.1:7070/v1/chat/completions … -d '{"model":"fast",…}'

# namespace——公开 `acme/fast` 就产生了 namespace `acme`
curl -s http://127.0.0.1:7070/acme/v1/chat/completions … -d '{"model":"fast",…}'

# Provider——只有那一个 Provider
curl -s http://127.0.0.1:7070/openai-main/v1/chat/completions … -d '{"model":"gpt-4o-mini",…}'
```

挂载点通过**给模型名加前缀**来收窄：`/acme/v1/messages` 配 `{"model":"fast"}` 解析的是
`acme/fast`。这是解析器自己的 `前缀/模型` 语法，不是第二条规则。已经带了前缀的名字原样
保留，因此客户端两种写法都行。

只有在剥掉前缀后剩下的部分本身也是本网关提供的 surface 时，前缀才会被剥掉。正是这条规则
挡住了一个叫 `backend-api` 的 Provider 吃掉 `/backend-api/codex/responses`——那是一条真实
的 Codex 路径。

## 错误

产品面的信封是

```json
{"error":{"code":"unknown_model","message":"unknown model `nope`"}}
```

| 状态码 | 含义 |
| --- | --- |
| `400` | 请求格式不对，或点名了无效的东西 |
| `401` | 凭据缺失、未知、被禁用、过期或已撤销 |
| `403` | 准入拒绝：权限，或 OAuth 操作基线 |
| `404` | 模型名、路由或那一行不存在 |
| `429` | 限流；cache 回答不了时同样在这里拒绝 |
| `5xx` | 本实例失败，或所有上游尝试都失败 |

5xx 的消息从不返回给调用方——它可能引用 DSN、一行数据或一个 header。返回的是 `code`，
文本进运维者的日志。

OAuth 端点用的是 RFC 6749 §5.2 的信封，因为 RFC 规定 `error` 是字符串，而在那里读到一个
对象的客户端分不清"重新登录"和"继续轮询"：

```json
{"error":"invalid_grant","error_description":"…"}
```

## 事后怎么找到这个请求

v4 **没有 `x-request-id` 响应 header**。请求自己的 id 随用量行和 capture 一起记录，而
调用方自己能看到的那一份在用户面：

```sh
curl -s http://127.0.0.1:7070/portal/api/usage    -H "Authorization: Bearer $GPROXY_KEY"
curl -s http://127.0.0.1:7070/portal/api/requests -H "Authorization: Bearer $GPROXY_KEY"
```

记录了什么、脱敏了什么、以及 HTTP 宿主还没暴露什么，见
[用量、日志与审计](/zh-cn/guides/observability/)。
