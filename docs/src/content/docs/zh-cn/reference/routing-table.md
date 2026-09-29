---
title: "路由与端点"
description: v4 完整的入站表、挂载点语法、OAuth 与管理路由、WebSocket surface、解析与失败转移预算。
---

下面的一切就是原生宿主的路由表，而 Workers 宿主挂载的是同一张。`gproxy-protocol` 刻意
不声明任何路径——哪个 URL 承接哪个操作是入口层拥有的 HTTP 约定——因此本页就是那一层的表。

## 网关自己的路由

四条，其余全部落到数据面。这和通常的安排正好相反，而且是刻意的：网关自己的路由是一份短
而已知的清单，别的都是本实例转发的、属于别人的 API。新增一个上游 surface 不该需要在这里
新增一条路由。

| 方法 | 路径 | 是什么 |
| --- | --- | --- |
| `GET` | `/healthz` | 存活与已发布的配置 revision；**不需要认证** |
| `GET` | `/publications/{id}` | 一份已发布的 body。id **就是**凭据，因此不问 key |
| — | `/admin/api/…` | 运维面 |
| — | `/portal/api/…` | 终端用户自己的面 |
| — | 其余一切 | 入站兜底 |

```sh
curl -s http://127.0.0.1:8787/healthz
```

```json
{"revision":2,"status":"ok"}
```

`/healthz` 读已发布的快照，既不碰数据库也不碰 cache，因此负载均衡器轮询它不会自己变成
压垮它的那份负载。

## 入站的顺序

1. **CORS 与客户端地址。** 预检在这里应答、绝不转发——它问的是本实例接受什么，上游对此
   没有意见。客户端地址在任何东西能记录它之前，按
   [可信代理规则](/zh-cn/reference/configuration/#可信代理规则)解析。
2. **挂载点**——路径里哪一段是本网关的。
3. **OAuth issuer**，在 `{mount}/v1/oauth/…`。排在数据面之前，因为这些路径是*本实例*的
   而不是某个上游的，而且它们用另一套错误信封应答。
4. **某个渠道的厂商服务路由**——Codex 的 `/backend-api/…`、Claude Code 的 `/api/…`。
   只在 Provider 或 namespace 挂载点上：匹配它需要一个渠道，而聚合挂载点没有点名渠道。
5. **数据面**，见下。
6. **console**，带 SPA 兜底，受配置开关控制。

走到尽头的一切都是 404。

## 数据面

请求体在做任何事之前（包括鉴权）先受 `settings.maxRequestBodyBytes` 约束：默认 50 MiB，
文件上传用 `maxUploadBodyBytes`，默认 512 MiB。

### 内容生成

| 方法 | 路径 | 操作 / 方言 |
| --- | --- | --- |
| `POST` | `/v1/responses` | `generate_content` / `openai` |
| `POST` | `/v1/chat/completions` | `generate_content` / `openai_chat` |
| `POST` | `/v1/messages` | `generate_content` / `claude` |
| `POST` | `/v1/messages/count_tokens` | `count_tokens` / `claude` |
| `POST` | `/v1beta/models/{model}:generateContent` | `generate_content` / `gemini` |
| `POST` | `/v1beta/models/{model}:streamGenerateContent` | `stream_generate_content` / `gemini` |
| `POST` | `/v1beta/models/{model}:countTokens` | `count_tokens` / `gemini` |
| `POST` | `/v1beta/models/{model}:embedContent` | `create_embedding` / `gemini` |
| `POST` | `/v1beta/models/{model}:batchEmbedContents` | `batch_create_embedding` / `gemini` |

前三条在 body 说 `"stream": true` 时变成 `stream_generate_content`。它们是**两个不同的
操作**——不同的规则、不同的结算——所以这个标志在入口就被读取，而不是留给渠道去发现。
Gemini 把它写在路径里，这也是那个方言有两行的原因。

### 模型

| 方法 | 路径 | 操作 / 方言 |
| --- | --- | --- |
| `GET` | `/v1/models` | `list_models` / `openai`，再 `claude` |
| `GET` | `/v1/models/{id}` | `get_model` / `openai`，再 `claude` |
| `GET` | `/v1beta/models` | `list_models` / `gemini` |
| `GET` | `/v1beta/models/{id}` | `get_model` / `gemini` |

### 其余

| 方法 | 路径 | 操作 / 方言 |
| --- | --- | --- |
| `POST` | `/v1/embeddings` | `create_embedding` / `openai` |
| `POST` | `/v1/moderations` | `guardian_classify` / `openai` |
| `POST` | `/v1/rerank` | `rerank` / `openai` |
| `POST` | `/v1/conversations` | `create_conversation` / `openai` |
| `POST` | `/v1/images/generations` | `create_image` / `openai` |
| `POST` | `/v1/images/edits` | `edit_image` / `openai` |
| `POST` | `/v1/audio/speech` | `create_speech` / `openai` |
| `POST` | `/v1/audio/transcriptions` | `create_transcription` / `openai` |
| `POST` | `/v1/audio/translations` | `create_translation` / `openai` |

### 文件

| 方法 | 路径 | 操作 / 方言 |
| --- | --- | --- |
| `GET` `POST` | `/v1/files` | `list_files` / `create_file`，`openai` 再 `claude` |
| `GET` `DELETE` | `/v1/files/{id}` | `retrieve_file` / `delete_file` |
| `GET` | `/v1/files/{id}/content` | `retrieve_file_content` |
| `GET` `POST` | `/v1beta/files` | Gemini 的写法 |
| `POST` | `/upload/v1beta/files` | `create_file` / `gemini` |
| `GET` `DELETE` | `/v1beta/files/{id}` | `retrieve_file` / `delete_file`，`gemini` |
| `GET` | `/v1beta/files/{id}:download` | `retrieve_file_content` / `gemini` |

### 视频

| 方法 | 路径 | 操作 |
| --- | --- | --- |
| `GET` `POST` | `/v1/videos` | `list_videos` / `create_video` |
| `GET` `DELETE` | `/v1/videos/{id}` | `retrieve_video` / `delete_video` |
| `GET` | `/v1/videos/{id}/content` | `download_video_content` |

只有 Sora 有的操作——remix、edit、extend、characters——**刻意缺席**。没有第二家厂商提供
它们，等有第二家的时候它们会回来。

### Realtime 与套接字

| 方法 | 路径 | 操作 / 方言 |
| --- | --- | --- |
| `POST` | `/v1/realtime/calls` | `create_realtime_call` / `openai` |
| `GET` | `/v1/realtime` | `connect_realtime` / `openai`——升级 |
| `GET` | `/v1/live` | 同上，WebRTC 写法 |
| `GET` | `/v1/live/{call_id}` | 把 call 放在路径里的续接 |
| `GET` | `/v1/responses/ws` | `generate_content` / `openai_responses_websocket` |
| `GET` | `/ws/v1beta/BidiGenerateContent` | `connect_realtime` / `gemini`——Gemini Live |

`POST /v1/realtime/calls` 是一个携带 SDP offer 的 HTTP multipart 请求，而续接它的握手
**完全不带 body**。活过升级的是 call id——在路径里，或在 `?call_id=` 里——引擎用它把套接字
钉到应答那个 offer 的凭证上。query 被原样转发，正是为了这个。

realtime 会话**绝不被转换**：只有同方言直通，因为一个能在响应中途继续说话的客户端，在
半双工方言里没有对应物。

**任何东西在被允许之前都不会被升级。** 先认证、再看握手形状、再准入、再是上游握手；
`101` 最后才写。因此每一次拒绝都是客户端读得到的 HTTP 应答——一个被接受又立刻关闭的套接字
不带状态、不带 body、也不带 code。

**被拒绝的上游握手原样转达。** 厂商的
`429 {"error":{"code":"insufficient_quota"}}` 比这个网关能编出来的任何 502 都值钱。

一个套接字持有它的并发租约直到它**关闭**，而不是到 `101` 被写出为止。跑一小时的会话就占
一小时的名额。

### 有歧义的那几条路径

`/v1/models`、`/v1/models/{id}` 和 `/v1/files` 被 OpenAI、Claude 和 Gemini 的 v1 surface
拼成了同一个样子。方言决定向转换器要哪一套协议类型，猜错就是客户端解析不了的 body。

判别依据是**客户端自己的认证 header**——`x-goog-api-key` 是 Gemini 的、`anthropic-version`
是 Claude 的——因为那是客户端主动提供的关于它自己的证据，而不是本网关发明的默认值。没有
证据时第一行胜出，而表的顺序让那一行是 OpenAI。

## 挂载点语法

| 路径 | 挂载点 | 收窄到 |
| --- | --- | --- |
| `/v1/messages` | 聚合 | 不收窄 |
| `/acme/v1/messages` | namespace `acme` | `acme/` 下的公开名称 |
| `/openai-prod/v1/messages` | Provider `openai-prod` | 那一个 Provider |

**namespace** 是带斜杠的公开模型名的第一段。**Provider** 挂载点用的是 Provider 的 `name`
——运维者取的标签，而不是行 id，因为没有人会把机器生成的 id 敲进客户端的 base URL。同名时
namespace 胜过 Provider。

**只有在剥掉前缀后剩下的部分也是一个被声明的 surface 时，前缀才会被剥掉。** 这条规则的
每一种简化都是错的：一个叫 `backend-api` 的 Provider 否则会吃掉
`/backend-api/codex/responses`——那是一条真实的 Codex 路径——把一次数据面调用变成一个并不
存在的挂载点。

**因此有歧义的路径一律倒向聚合挂载点。** 当第一段确实点名了什么、但剩下的部分不是本网关
提供的 surface 时，路径原样保留。丢掉一个挂载点是运维者看得见的 404；错认一个挂载点则是
把请求发给了错误的上游。

挂载点通过**给模型名加前缀**来收窄，这是解析器自己的语法而不是第二条规则。已经带了前缀的
名字原样保留。

:::note[一个已知的缺口]
**不带模型**的操作在 Provider 挂载点上被收窄到那个 Provider 的*渠道*，而不是那个
Provider。`GET /p1/v1/models` 会列出 `p1` 所在渠道上、调用方能触达的每个 Provider 的模型。
补上它需要请求形状里还没有的一个字段。
:::

## OAuth issuer

相对某个挂载点前缀（`""`、`/acme`、`/openai-prod`）：

```text
GET  {prefix}/v1/oauth/authorize    同意页交接（浏览器则 302 到 `/console/authorize`）
POST {prefix}/v1/oauth/authorize    用户的决定
POST {prefix}/v1/oauth/token        code、refresh 与设备授权
POST {prefix}/v1/oauth/device/code
POST {prefix}/v1/oauth/revoke
GET  {prefix}/v1/.well-known/oauth-authorization-server
GET  /.well-known/oauth-authorization-server{prefix}/v1   （RFC 8414 §3.1）
```

issuer 标识是 `{origin}{prefix}/v1`，这也是聚合挂载点是 `https://host/v1` 而不是
`https://host` 的原因：这样一条规则就产出全部三个挂载点。RFC 8414 要求这个标识恰好是
客户端取到文档的那一个，而只有宿主知道请求是从哪个挂载点进来的，所以它是被传进去的而
不是算出来的。

v3 把它们放在根路径。移到 `/v1` 之下让前缀规则与 `/v1/messages` 同构，而这次搬迁很便宜
——v3 根本没实现发现端点，客户端本来就在硬编码地址。

## 管理路由

每个身份家族都有同样的五条路由，由一份声明生成，因此一个家族不可能不小心只有四条：

```text
GET    /admin/api/{family}           列表（分页与过滤在 query 里）
POST   /admin/api/{family}           创建
GET    /admin/api/{family}/{id}      读取
PATCH  /admin/api/{family}/{id}      更新
DELETE /admin/api/{family}/{id}      删除    → 204
```

**身份**家族：`users`、`api-keys`、`organizations`、`teams`、`permissions`、
`rate-limits`。组织和团队成员通过各自的成员接口管理。
`oauth-clients` 有前四条，外加 `POST …/{id}/retire` 取代删除——一个客户端签发过的授权还
指着它。

**配置**家族在同样的五条之上**多一个批量**，因为它们每一个都接受批量，而一次批量无论
点名多少行都是一个 revision 提交：

```text
POST /admin/api/{family}/batch
     [{"create": …}, {"update": {"id": …, "patch": …}}, {"delete": "id"}]
```

`providers`、`credentials`、`models`、`provider-models`、`routes`、`route-members`、
`connection-profiles`、`rule-sets`、`rules`、`provider-rule-sets`、
`operation-rules`、`operation-endpoints`、`quotas`、`price-rules`、`price-rates`、
`price-tiers`。

五条之外还有：`settings`、凭证操作（`reveal`、`status`、`refresh`、`quota`、
`quota-probe`、`quota-reset`、`health-reset`、`limits`）、
`models/{discover,discover/apply,test}`、`rule-sets/{id}/rules`、
`rule-sets/{id}/rule-presets/{preset}`、`providers/{id}/routing-defaults/reset`、
`quotas/status`、`quotas/{id}/{reset,limit-reset}`、`export`、`import`、
`connectivity/test`、`channels`、`tls-presets`、`rule-presets`、
`default-model-catalog`、`tokenizer-vocabs`、`tokenizer-auth`、`session`、`sessions`
和 `audit`。

v4 的管理请求与数据结构已有变化，旧脚本应按本页接口重新核对。实例级用量和日志接口见[用量、日志与审计](/zh-cn/guides/observability/)。

中间件依次是：认证 → **检查管理范围与操作权限** → 对不安全的 cookie 请求做同源校验 → 操作 →
为每个不是读的方法写一行审计。它是一个 route layer，因此未知的 `/admin/api/*` 路径是一个
根本不碰数据库的 404。

## 用户面路由

```text
POST   /portal/api/login             用户名 + 密码 → 会话 cookie 与令牌
POST   /portal/api/logout
GET    /portal/api/context           调用方、他们的成员关系、他们的特性开关
GET    /portal/api/models            全部公开名称，各自标注 `permitted`
GET    /portal/api/usage             他们自己的花费
GET    /portal/api/quota             他们自己的预算窗口
GET    /portal/api/requests          他们的近期请求（若已启用）
GET    /portal/api/sessions
GET    /portal/api/keys              （+ POST、DELETE …/{id}、POST …/{id}/rotate、GET …/{id}/secret）
GET    /portal/api/oauth-sessions    （+ DELETE …/{id}）
POST   /portal/api/password
GET    /portal/api/oauth/device      查询待批准的设备码（?userCode=），供 `/console/device` 使用
POST   /portal/api/oauth/device      批准或拒绝；需要登录会话或带管理权限的 key
```

这里的守卫要求一个已认证的调用方，**仅此而已**。没有角色检查，因为没有东西可检查：用户面
上的每个操作按构造就限定在构造它的那个调用方上，而且没有一个接收用户 id。检查可能被忘掉，
不存在的参数不会。

`login` 和 `logout` 在守卫之外——前者跑在有调用方之前，后者必须对一个已经过期的会话也有效，
否则浏览器会攥着一个永远丢不掉的 cookie。

## 解析与失败转移预算

四种名字形式与排序见[模型与路由](/zh-cn/guides/models/)。预算：

- 它是路由自己的 `maxAttempts`，被 `settings.maxAttempts` 钳住；
- 它**跨目标共享**：每个目标最多拿到自己凭证数那么多次、且不超过剩余额度，因此一个计划的
  上游调用次数不会超过它的预算；
- 只有"另一个 Provider 可能救得回来"的失败才换目标——没有可用凭证、凭证已死、续接钉在别的
  实例上、任何渠道或传输错误，或 401／403／429／5xx 应答；
- 预算耗尽、被禁止、被取消、转换错误，以及 store 或 cache 故障，都就地停止；
- 没有目标可换时，**最后一个应答原样返回**。最后一个 Provider 的 429 就是调用方的 429，
  不会被换成别的错误。

请求体缓冲一次以便重放。超过 `maxRequestBodyBytes` 的流式体保持流式，计划随之裁剪为单个
目标：大文件上传不值得为了失败转移而全部读进内存。
