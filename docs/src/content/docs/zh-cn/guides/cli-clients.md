---
title: CLI 客户端
description: 把 Codex CLI 和 Claude Code 指向 GPROXY：渠道声明的厂商服务路由、三种视图，以及本实例运行的 OAuth issuer。
---

有些厂商 CLI 要说的不只是推理端点。它们还要从厂商的控制面取账号资料、用量窗口、插件、
任务和文件，而且拿不到就不干活。

因此渠道可以声明一张**服务路由表**：它认识的控制面路径，以及每条路径返回什么。宿主会在
尝试数据面之前，把进来的路径拿去和那张表匹配。

## 怎么到达它们

控制面路径是厂商特有的，所以只在 **Provider 或 namespace 挂载点**上匹配。匹配一条服务
路由需要一个渠道，而聚合挂载点没有点名任何渠道。

```text
https://gproxy.example/codex/backend-api/wham/profiles/me
                       ^^^^^ 名为 "codex" 的 Provider（或 namespace）
```

## 三种视图

`x-gproxy-view` 说明调用方想要什么。不带表示 `caller`，而那也是普通成员唯一能要的视图。

| Header | 回答什么 |
| --- | --- |
| `x-gproxy-view: caller` | **这个调用方**的事实——他们的用量窗口、预算链、他们自己的调用创建的资源。身份、用量和设置类请求绝不触达厂商。 |
| `x-gproxy-view: pool` | 同上，但在目标里的每把凭证上聚合。 |
| `x-gproxy-view: credential:{id}` | 原始转发。点名凭证自己的鉴权发往上游，厂商的应答原样回来。 |

谁能要哪一种由归属决定，不是由某个开关决定。目标是调用方在该 Provider 上**可见的**凭证
集合——和同一把 key 的模型调用能花的是同一批——而只有当调用方是实例管理员，或者是拥有
该集合中**每一把**凭证的那个组织或团队的 `Admin` 成员时，他们才是这个目标的管理员。
其余都是成员，成员只能要 `caller`；`pool` 和 `credential:{id}` 返回 `403`。

"每一把"不是形式主义：管理一个组织，不应该渲染出一个同时包含另一个组织的池。无归属的
凭证按构造就没有管理员——没有人是"什么都不是"的 admin 成员——所以在一个凭证全部共享的
单租户实例上，只有实例管理员能触达 `pool` 和 `credential`。

**签发 key 在每种视图下都被拒绝。** 一把挂在共享订阅上的长期 API key，正是共享网关存在
要避免交出去的那种凭据。

表里没有的路径返回 `404`，除非视图是 `credential:{id}`。

## 什么不被计量

厂商服务调用**不占限流租约**——给一次 profile 拉取扣一个窗口，等于花掉调用方留给真正花钱
流量的额度——也**不写 capture 记录**。引擎不给服务写上游记录，因此一条下游记录将无处可指，
读起来会像一个没到达任何上游的请求。

它确实会带上预算链，因为 `caller` 视图要用它渲染调用方自己的窗口。报告一个配额不等于
花掉一个。

厂商服务调用与模型请求共用同一套取消机制，覆盖凭据准备、上游调用和 HTTP 响应体读取。
WebSocket 服务的 token 覆盖握手阶段，已建立的连接由宿主管理。服务仍不占限流租约，
也不产生模型用量。

## Codex CLI

`codex` 渠道声明了 CLI 在 Responses 之外对 ChatGPT 后端发起的调用。wire 事实依据 CLI
自己在 `samples/codex` 下的源码：

| 家族 | 路径 |
| --- | --- |
| 插件与 MCP | `/backend-api/ps/**` |
| 账号、设置、用量、任务、环境、远程控制 | `/backend-api/wham/**` |
| 上传 | `/backend-api/files/**` |
| 工作区插件共享 | `/backend-api/public/plugins/**` |
| 启动时的身份探测 | `GET /v1/user-auth-credential/whoami` |

CLI 用两种方式寻址同一批端点——ChatGPT 主机上是 `/backend-api/wham/**`，Codex API 主机上
是 `/api/codex/**`。两种拼法，以及裸的 `/codex/**` 和 `/ps/**` 挂载，在查表之前都被归一
到 `/backend-api/**`，所以客户端选哪种主机风格不是一个配置问题。

`/backend-api/wham/remote/control/server` 是一个 **WebSocket**，也是唯一一条会升级的服务
路由。Codex 直接拒绝被合成的视图，因此由该凭证的管理员发出的
`x-gproxy-view: credential:{id}` 是唯一的通路。

表没有分类的路径落进 `/backend-api/{*path}` 前缀行，原样转发，非 2xx 也一样。

## Claude Code

`claudecode` 渠道声明了 CLI 在 Messages 之外发起的、OAuth 作用域内的 `/api/**` 调用：
profile、validate、roles、bootstrap、用量、策略限额、账号设置、文件上传下载、组织连接器、
插件与 skill、引导与计费控制、桌面更新跳转，以及 Claude Design surface。

它们全部解析到 `api.anthropic.com`；`claude.ai` 只提供 cookie 登录，因此渠道在这里不查它。

在 `caller` 与 `pool` 下，身份、用量与设置类答案由本实例已知的事实合成，其 id 是对
Provider 和调用方身份的稳定哈希。关于那个共享账号，既不编造也不泄漏：CLI 显示的账号、
组织和套餐是 GPROXY 的合成值，不是上游账号的。

## GPROXY 作为 OAuth issuer

用 OAuth 登录的 CLI 可以登录到**本实例**，而不是登录到厂商。这和 sdk 的 `login()` 是两件
事，后者是以客户端身份去讲别人的 OAuth：

```text
  Claude Code ──authorize/token──▶ gproxy   （授权服务器）
                                     │
                                     └──login()──▶ OpenAI   （客户端）
```

端点挂在每个挂载点之下，因此客户端发现的就是它正在对话的那一个：

```text
GET  {mount}/v1/oauth/authorize
POST {mount}/v1/oauth/authorize
POST {mount}/v1/oauth/token
POST {mount}/v1/oauth/device/code
POST {mount}/v1/oauth/revoke
GET  {mount}/v1/.well-known/oauth-authorization-server
GET  /.well-known/oauth-authorization-server{mount}/v1
```

```sh
curl -s http://127.0.0.1:8787/v1/.well-known/oauth-authorization-server
```

```json
{"issuer":"http://127.0.0.1:8787/v1",
 "authorization_endpoint":"http://127.0.0.1:8787/v1/oauth/authorize",
 "token_endpoint":"http://127.0.0.1:8787/v1/oauth/token",
 "device_authorization_endpoint":"http://127.0.0.1:8787/v1/oauth/device/code",
 "revocation_endpoint":"http://127.0.0.1:8787/v1/oauth/revoke",
 "response_types_supported":["code"],
 "grant_types_supported":["authorization_code","refresh_token",
   "urn:ietf:params:oauth:grant-type:device_code"],
 "code_challenge_methods_supported":["S256"]}
```

v3 把它们放在根路径；v4 移到 `/v1` 之下，让前缀规则与 `/v1/messages` 同构，只需要一份实现。

### 这个 issuer 不让步的规则

- **永远 PKCE S256。** `plain` 被拒绝，缺少 challenge 也被拒绝。每个注册的客户端都是
  public、没有 secret 可证明自己，所以 verifier 是泄漏的授权码与令牌之间唯一的东西。
- **redirect URI 精确匹配。** 不是前缀、不是通配符、也不是"同源"。每一条更宽松的规则都会
  接受一个攻击者控制的 URL。
- **授权码、refresh token 与设备码一次性**，在一个原子批里被消费并替换，因此两次兑换无论
  怎么交错都不可能都成功。
- **被重放的授权码或 refresh token 撤销整个家族**——授权、它的内部 key，以及它签发过的
  每一个令牌。没有办法把"应答丢了"和"凭据被偷了"区分开，所以一律按泄漏处理。合法客户端
  重新登录一次；小偷不能。设备流是例外，因为轮询客户端按设计就会重发它的码。

### 一个 OAuth 调用方能做什么

access token 是用户交给**别人的二进制**的一份凭据。除非该客户端被写进实例的
`oauth.cliClientIds`，它只能列模型、取模型、数 token、生成、流式和压缩。其他一律 `403`，
而且这个限制对实例管理员的账号同样生效——管理员的账号恰恰是最不能让第三方令牌变成管理
凭据的那一个。

把一个客户端写进 `cliClientIds`，等于运维者接受它在整个 API 上代表这个用户说话。

一个 OAuth 调用方也不能签发、轮换、揭示或删除 key，不能修改账号密码。

## 会话亲和性

多轮 CLI 在"一次对话停在一把凭证上"时工作得最好。GPROXY 从**实际发出**请求的客户端读取
对话标识，绝不从它即将被转发到的上游读——一个被转发到别的渠道的 Claude Code 请求，读的
仍然是 Claude Code 的字段。

阶梯，第一个非空值胜出：

1. `x-gproxy-session-id`，网关自己的 header；
2. `thread-id`、`session-id`、`x-claude-code-session-id`、`x-conversation-id`、
   `x-grok-session-id`；
3. 入站 body 自己的字段——Responses 的 `client_metadata.thread_id`/`session_id`、
   Claude 在 JSON 编码的 `metadata.user_id` *内部*的 `session_id`、Gemini 的
   `request.session_id` 与 `request.sessionId`；
4. 对话稳定前缀的 sha256 指纹；
5. 请求 id，诚实地标注为兜底，而不是冒充成一个会话。

不会有任何东西在任意 JSON 里递归找名为 `session_id` 的字段；而请求级、轮次级和缓存级
标识——`x-grok-req-id`、`user_prompt_id`、`prompt_cache_key`、`previous_response_id`
——**绝不是**会话。

网关 header 在请求发往上游前被摘除，客户端发来的那一份也被摘除：客户端不能通过发送网关
用来命名会话的那个 header 去挑别人的对话。缺少会话 header 从不导致请求被拒。
