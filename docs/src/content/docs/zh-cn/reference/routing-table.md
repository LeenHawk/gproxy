---
title: "路由与端点"
description: "入口模式、每个操作在各协议族下的路径、模型选择与解析顺序、故障转移预算，以及发往上游前剥离的请求头"
---

到达网关的每个请求都会先对照同一份操作注册表
（`crates/gproxy-protocol/src/specs/`）分类，再解析为有序的上游候选列表，
最后经过同一个结算漏斗。本页是这几个步骤的参考。

## 聚合模式与命名模式

GPROXY 接受两种路径形态。

| 模式 | 路径形态 | 由什么选择后端 |
| --- | --- | --- |
| 聚合 | 已声明的操作路径：`/v1/chat/completions`、`/v1/messages`、`/v1beta/models/{model}:generateContent` 等 | 请求中的模型名，经别名和路由解析。 |
| 命名 | `/{name}/...`，其余部分必须是已声明的操作路径或通道服务面：`/codex/v1/responses`、`/codex/backend-api/codex/responses`、`/codex/oauth/token` | 路径的第一段。 |

第一段满足以下任一条件即视为名称：**路由命名空间**（公开模型名中 `/`
之前的部分，如 `openai/gpt-5`，不区分大小写）、**路由名称**，或
**Provider 名称**，按此顺序检查。若该段不是已知名称，或其余部分不是已声明
的路径，整条路径按聚合模式处理，并因分类失败返回 400。

- 命名到命名空间：请求的模型在该命名空间内查找，`/{ns}/v1/...` 配合
  `"model": "gpt-5"` 解析为 `ns/gpt-5`。
- 命名到路由：无论请求体中的模型是什么，都由该路由的成员服务；发往上游的
  是各成员的 `upstream_model`。
- 命名到 Provider：绕过路由。计划是该 Provider 的全部已启用凭证，请求的模
  型名在经过 Provider 级别名和变体处理后原样发往上游。模型列表遵循该
  Provider 的路由规则，而不是从快照回答。

服务面（Codex CLI 的 `backend-api` 路由、Claude Code 的控制面路由）只能在
命名模式下访问，因为它们由通道而不是操作注册表声明。它们与其他请求一样经
过认证、准入和结算。

## 客户端如何指定模型

| 协议族 | 模型来自 | 协议族识别 |
| --- | --- | --- |
| OpenAI Chat、Responses 及其他所有 `/v1/...` 操作 | 请求体 `model`；realtime 呼叫也接受 `session.model` | 路径。 |
| Claude Messages | 请求体 `model` | 路径；在共享路径（`/v1/models`、`/v1/models/{id}`）上，`x-api-key`、`anthropic-version`、`anthropic-beta` 任一存在即视为 Claude 调用方。 |
| Gemini | 路径段 `{model}`：`/v1beta/models/{model}:generateContent` | 路径。 |
| 获取模型 | 路径段 `{id}` 或 `{model}` | 路径。 |

流式检测按入口进行：请求体中的布尔 `stream`（Chat、Responses、Messages、
图像生成），JSON 或 multipart 字段中的 `stream`（图像编辑、转写），
`stream_format: "sse"`（语音），或端点本身（`:streamGenerateContent`、
guardian、WebSocket 升级）。`stream: true` 会把 `generate_content` 提升为
`stream_generate_content`。Gemini 流默认是增量 JSON 数组，除非请求带
`?alt=sse`。

路由规则可以只改变流式性：把目标指向兄弟操作即可。`generate_content` →
`stream_generate_content` 强制上游流式，并把事件流折叠成一个完整对象返回给
非流式客户端（Kiro 这类只会说事件流的上游就是这样服务的）。
`stream_generate_content` → `generate_content` 则向上游取一个完整对象，再合成
客户端协议的事件流。原生宿主会立即打开该流，在等待上游期间每 15 秒发一次
keepalive（Claude 用 `ping` 事件，SSE 用注释，Gemini 数组流用 JSON 空白）；
此后上游失败会以该协议的错误事件送达。边缘宿主则在结束时缓冲并合成。

## 操作

结算模式：**response** 按响应或流尾部的用量计费；**free** 从不计费；
**session end** 在 realtime 会话关闭时计费；**completed status** 在轮询的视
频任务报告 `completed` 时计费一次。亲和 **session** 把一段对话固定到一个凭
证；**resource** 把后续调用固定到创建该文件、视频、角色或呼叫的凭证。协议
族 ID 为 `openai`、`claude`、`gemini`；内容生成的 kind 为 `openai_chat`、
`openai_responses`、`openai_responses_websocket`、`claude_messages`、
`gemini_generate_content`。

### 模型与 Token

| 操作 | 方法与路径（协议族） | 结算 | 亲和 |
| --- | --- | --- | --- |
| `list_models` | `GET /v1/models`（openai、claude）；`GET /v1beta/models`（gemini） | free | — |
| `get_model` | `GET /v1/models/{id}`（openai、claude）；`GET /v1beta/models/{model}`（gemini） | free | — |
| `count_tokens` | `POST /v1/messages/count_tokens`（claude）；`POST /v1/responses/input_tokens`（openai）；`POST /v1beta/models/{model}:countTokens`（gemini） | free | — |

### 内容生成、Compact 与 Memories

| 操作 | 方法与路径（kind） | 结算 | 亲和 |
| --- | --- | --- | --- |
| `generate_content` | `POST /v1/chat/completions`（openai_chat）；`POST /v1/responses`（openai_responses）；`POST /v1/messages`（claude_messages）；`POST /v1beta/models/{model}:generateContent`（gemini_generate_content） | response | session |
| `stream_generate_content` | 以上四个路径配合 `stream: true`；`POST /v1beta/models/{model}:streamGenerateContent`；`GET /v1/responses` WebSocket 升级（openai_responses_websocket） | response | session |
| `guardian_review` | `POST /v1/guardian`（openai_responses，始终流式） | response | session |
| `guardian_classify` | `POST /v1/guardian-classifier`（openai_responses，始终流式） | response | session |
| `compact_content` | `POST /v1/responses/compact`（openai） | response | session |
| `summarize_memory` | `POST /v1/memories/trace_summarize`（openai） | response | — |

### Embeddings、Rerank 与搜索

| 操作 | 方法与路径（协议族） | 结算 | 亲和 |
| --- | --- | --- | --- |
| `create_embedding` | `POST /v1/embeddings`（openai）；`POST /v1beta/models/{model}:embedContent`（gemini） | response | — |
| `batch_create_embedding` | `POST /v1beta/models/{model}:batchEmbedContents`（gemini） | response | — |
| `rerank` | `POST /v1/rerank`（openai） | response | — |
| `web_search` | `POST /v1/alpha/search`（openai） | response | — |

### 图像与音频

| 操作 | 方法与路径（协议族） | 结算 | 亲和 |
| --- | --- | --- | --- |
| `create_image` | `POST /v1/images/generations`（openai，`stream` 标志）；`POST /v1beta/models/{model}:predict`（gemini） | response | — |
| `edit_image` | `POST /v1/images/edits`（openai，JSON 或 multipart 中的 `stream`） | response | — |
| `create_speech` | `POST /v1/audio/speech`（openai，`stream_format` 为 `sse` 时流式） | response | — |
| `create_transcription` | `POST /v1/audio/transcriptions`（openai，JSON 或 multipart 中的 `stream`） | response | — |
| `create_translation` | `POST /v1/audio/translations`（openai） | response | — |

### 文件

| 操作 | 方法与路径（协议族） | 结算 | 亲和 |
| --- | --- | --- | --- |
| `create_file` | `POST /v1/files`（openai）；`POST /upload/v1beta/files`（gemini） | free | resource `file` |
| `list_files` | `GET /v1/files`（openai）；`GET /v1beta/files`（gemini） | free | resource `file` |
| `retrieve_file` | `GET /v1/files/{id}`（openai）；`GET /v1beta/files/{id}`（gemini） | free | resource `file` |
| `retrieve_file_content` | `GET /v1/files/{id}/content`（openai）；`GET /v1beta/files/{id}:download` 和 `GET /download/v1beta/files/{id}:download`（gemini） | free | resource `file` |
| `delete_file` | `DELETE /v1/files/{id}`（openai）；`DELETE /v1beta/files/{id}`（gemini） | free | resource `file` |

### 视频

| 操作 | 方法与路径（协议族） | 结算 | 亲和 |
| --- | --- | --- | --- |
| `create_video` | `POST /v1/videos`（openai）；`POST /v1beta/models/{model}:predictLongRunning`（gemini） | free | resource `video` |
| `retrieve_video` | `GET /v1/videos/{id}`（openai）；`GET /v1beta/operations/{id}` 和 `GET /v1beta/models/{model}/operations/{id}`（gemini） | completed status | resource `video` |
| `list_videos` | `GET /v1/videos`（openai） | free | resource `video` |
| `delete_video` | `DELETE /v1/videos/{id}`（openai） | free | resource `video` |
| `download_video_content` | `GET /v1/videos/{id}/content`（openai） | free | resource `video` |
| `remix_video` | `POST /v1/videos/{id}/remix`（openai） | free | resource `video` |
| `edit_video` | `POST /v1/videos/edits`（openai） | free | resource `video` |
| `extend_video` | `POST /v1/videos/extensions`（openai） | free | resource `video` |
| `create_video_character` | `POST /v1/videos/characters`（openai） | free | resource `video_character` |
| `get_video_character` | `GET /v1/videos/characters/{id}`（openai） | free | resource `video_character` |

视频任务在 `retrieve_video` 的轮询首次报告 `completed` 时计费；结算跨轮询去
重，因此创建和之后的轮询本身不产生费用。

### Realtime

| 操作 | 方法与路径（协议族） | 结算 | 亲和 |
| --- | --- | --- | --- |
| `create_realtime_call` | `POST /v1/realtime/calls`（openai） | session end | resource `realtime_call` |
| `connect_realtime` | `GET /v1/realtime` WebSocket 升级（openai） | session end | session |

## 本地回答

聚合或命名空间模式下的 `list_models` 和 `get_model` 由控制面快照回答：公开
模型，加上为开启了 `auto_refresh_models` 的 Provider 并发拉取的上游目录，合
并进列表。`count_tokens` 以及路由规则（或通道默认）为 `local` 的其他单元由
网关用分词器阶梯回答。本地回答仍会认证、通过权限和限流检查，并完成准入与
请求遥测。free 操作从不预扣配额，也不写用量记录。

## WebSocket 入口

升级意图通过 `sec-websocket-*`、`Upgrade: websocket` 或 `Connection: upgrade`
头识别。两个操作接受升级：`GET /v1/responses`（WebSocket 上的 Responses，是
Responses 线路形态的一种封装，叠加在 Responses 的转换对之上）和
`GET /v1/realtime`。通道可以声明自己的 WebSocket 服务面，通过命名模式访
问。匹配升级入口却没有升级的请求，或反之，都会被拒绝。升级在原生宿主以及
Cloudflare 和 Deno 上可用；Netlify 返回 501。

## 解析顺序

对于指定了模型的请求，控制面按以下顺序解析。该顺序由
`crates/gproxy-core/src/tests/pricing.rs` 中的测试覆盖。

1. **别名。** 先应用全局别名（没有 Provider 的 `aliases` 行）；当模式指向某
   个 Provider 时，再应用该 Provider 的别名。别名精确匹配；按 `(priority, id)`
   排序的第一条启用行生效。
2. **变体后缀。** 若别名解析后的名称是某个公开模型声明的变体
   （`variants_json`），预设会从末尾剥离并写入请求体：
   `-thinking-none|low|medium|high|xhigh|adaptive` 变为 `reasoning_effort`
   （Chat）、`reasoning.effort`（Responses）、`thinking`（Claude；预算
   1,024 / 10,240 / 32,768 token、`adaptive` 或 `disabled`），或
   `generationConfig.thinkingConfig.thinkingLevel`（Gemini）；
   `-tier-priority|default|scale|flex|auto` 和 `-fast` 变为 `service_tier`。预设
   可以叠加（`-thinking-high-tier-flex`）。未声明为变体的名称上的后缀不做处
   理，按原样查找。
3. **路由。** 解析后的名称在 `exposed_models` 中查找，命名空间内则查找
   `ns/name`。未知名称返回 404。
4. **成员到计划。** 其凭证对该模型（或 `*`）被标记为不可用的成员被丢弃。其
   余成员按层级、健康状态（健康先于性能下降）、成员权重、凭证权重排序。在
   第一层内，按路由计数器以权重比例选出成员，再在该成员内选出凭证：Provider
   策略 `round_robin` 推进按成员的计数器，`sticky` 把会话或 API 密钥亲和键哈
   希到固定槽位。选中的目标移到最前；其余成员保持排序作为故障转移目标。轮
   转是计数器而非随机，连续请求会依次走过加权槽位。
5. **凭证。** 每个目标是 `(Provider, 凭证, 上游模型)`。Provider 设置、代理
   （凭证优先于 Provider，再优先于全局）和 TLS 指纹覆盖随目标一起传递。

## 故障转移预算

预算为 `min(route.max_attempts, GPROXY_MAX_ATTEMPTS)`；命名到 Provider 时为
`min(凭证数量, GPROXY_MAX_ATTEMPTS)`。只有实际发送才计入预算。以下情况跳过
目标而不消耗预算：

- 其凭证在本次请求中已被标记为不可用（秘密被拒、刷新失败，或上游返回
  `CredentialDead` 判定）；
- 资源亲和把请求固定到该 Provider 的另一个凭证；
- Provider 对该 `(operation, kind)` 的路由规则是 `unsupported`，或要求的转换
  对不存在；
- 凭证自身当前分钟的 RPM 或 TPM 上限已达到。

已发送的尝试若判定为 `Retryable` 或 `CredentialDead`，转到下一个目标；
`Success` 和 `Terminal` 返回给客户端。一次都未能发送时，响应为 500（没有目
标支持该操作）、400（不支持）、429（所有凭证都被限流）或 502（没有凭证）；
全部尝试失败时为 502 `all upstream attempts failed`。每个响应都带
`x-request-id`，在请求审计中把各次尝试关联起来。

## 发往上游前剥离的请求头

在入口，任何通道看到请求之前，GPROXY 移除 `x-gproxy-session-id`（它自己的会
话亲和提示）、逐跳头（`connection`、`content-length`、`keep-alive`、
`proxy-authenticate`、`proxy-authorization`、`proxy-connection`、`te`、
`trailer`、`transfer-encoding`、`upgrade`，以及 `Connection` 指名的任何头），
以及调用方的凭据和转发头（`accept-encoding`、`api-key`、`authorization`、
`cookie`、`forwarded`、`host`、`via`、`x-api-key`、`x-forwarded-for`、
`x-forwarded-host`、`x-forwarded-proto`、`x-goog-api-key`、`x-real-ip`）。
query 参数 `access_token`、`api_key`、`key` 和 `x-api-key` 也会被移除。

发往上游时，请求按白名单重建：`accept` 和 `content-type`、通道声明的名称，
以及 Provider 的 `traffic_policy` 设置。实例全局元数据黑名单中的名称或
`prefix-*` 模式即使在白名单中也会被移除。query 参数同样遵循通道白名单。之
后通道加入自己的认证信息。返回给客户端的响应头以同样方式过滤：基础列表
（`accept-ranges`、`allow`、`cache-control`、`content-disposition`、
`content-encoding`、`content-range`、`content-type`、`etag`、`expires`、
`last-modified`、`link`、`location`、`retry-after`、`vary`）加通道追加项，
且永不包含 `alt-svc`、`server`、`set-cookie`、`set-cookie2`、`via` 或
`www-authenticate`。

## 路由规则

Provider 的某个单元是直通、转换为其他 kind、本地回答还是不支持，由以
`(operation, kind)` 为键的按 Provider 矩阵决定；它从通道默认值播种，可在规
则工作区编辑。见[路由规则与规则集](/zh-cn/guides/rules/)。
