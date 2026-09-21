# gproxy-host-axum

[English](README.md) | 简体中文

GPROXY v4 的原生 HTTP 宿主：架在 [`gproxy-app`](../gproxy-app) 之上的一层
[axum](https://docs.rs/axum) 路由。它是一层**传输绑定**——把字节变成
`gproxy-app` 暴露的带类型调用，再把结果变回字节，自己不含任何产品行为。

仅限原生。它依赖 axum、hyper 与 tokio，三者都不能在
`wasm32-unknown-unknown` 上运行；边缘部署是另一个宿主，跑同一套操作。

```rust,ignore
let app = Arc::new(App::new(gproxy, config));
app.reload_all().await?;
let router = gproxy_host_axum::router(HostState::new(app));
axum::serve(
    listener,
    // 必须：没有它就拿不到对端地址，而未知对端一律不被信任。
    router.into_make_service_with_connect_info::<SocketAddr>(),
).await?;
```

## 路由表

| 方法 | 路径 | 含义 |
|---|---|---|
| `GET` | `/healthz` | 存活与已发布的配置 revision；不鉴权 |
| `GET` | `/publications/{id}` | 已发布的内容；id **本身就是**凭据，所以不要 key |
| — | `/admin/api/…` | 运维面：每个操作一条显式 `MethodRouter` |
| — | `/portal/api/…` | 终端用户面，外加 `login` 与 `logout` |
| — | 其余一切 | ingress 兜底，顺序见下 |

### `/admin/api`

每个身份家族都有同样的五条路由，由一个宏生成——这样一个家族不可能只写了四条，
也不可能某一条用了别的方法：

```
GET    /admin/api/{family}           列表（分页与筛选在 query 里）
POST   /admin/api/{family}           创建
GET    /admin/api/{family}/{id}      读取
PATCH  /admin/api/{family}/{id}      更新
DELETE /admin/api/{family}/{id}      删除 → 204
```

`{family}` 取 `users`、`api-keys`、`organizations`、`teams`、`permissions`、
`rate-limits`、`subscriptions`、`pools`、`pool-members`、`plans`、
`plan-limits`。`oauth-clients` 只有前四条，第五条换成
`POST …/{id}/retire`：它签发过的 grant 仍然指着它，所以只能退役不能删。五条之外：

```
GET    /admin/api/session                      本次请求是谁
DELETE /admin/api/session                      结束会话并清 cookie
POST   /admin/api/users/{id}/password          设置
DELETE /admin/api/users/{id}/password          清除
PUT    /admin/api/users/{id}/allowlist         OAuth client 允许列表
GET    /admin/api/users/{id}/sessions          他在哪些地方登录着
DELETE /admin/api/users/{id}/sessions          全部登出
POST   /admin/api/api-keys/{id}/rotate
GET    /admin/api/api-keys/{id}/secret         读出保留的明文
GET    /admin/api/organizations/{id}/members   （及 POST，与 …/{userId} 的 GET/PUT/DELETE）
GET    /admin/api/teams/{id}/members           （同上）
GET    /admin/api/sessions                     全部会话
DELETE /admin/api/sessions/{id}
GET    /admin/api/audit                        审计
```

中间件顺序：认证 → **要求实例管理员** → 非安全方法且是 cookie 调用者时校验同源 →
执行操作 → 非读方法写一行审计。它是 `route_layer`，所以未命中的
`/admin/api/*` 是一个连数据库都不碰的 404。

审计的 action 由命中的路由推出（`admin.api_keys.rotate`、
`admin.users.update`），新路由因此不可能忘记给自己命名。

### `/portal/api`

```
POST   /portal/api/login             用户名 + 密码 → 会话 cookie 与 token
POST   /portal/api/logout            结束会话并清 cookie
GET    /portal/api/context           调用者、他的组织/团队、他的订阅
GET    /portal/api/models            他能调哪些模型
GET    /portal/api/usage             他自己的用量
GET    /portal/api/quota             他自己的额度窗口
GET    /portal/api/requests          他最近的请求（若开关打开）
GET    /portal/api/sessions          他在哪些地方登录着
GET    /portal/api/keys              （及 POST）
DELETE /portal/api/keys/{id}         （及 POST …/rotate、GET …/secret）
GET    /portal/api/oauth-sessions    （及 DELETE …/{id}）
POST   /portal/api/password          修改密码，需证明当前密码
```

这里的守卫只要求"已认证"，**再无其他**。不做角色判断是因为没有可判断的东西：
`Portal` 上的每个操作都以构造它的那个 caller 为界，没有任何一个方法接受 user id。
`login` 与 `logout` 在守卫之外——前者跑在"还没有 caller"的时刻，后者必须对已经
过期的会话也生效，否则浏览器会攥着一个永远丢不掉的 cookie。

### ingress 的顺序

1. **CORS 与客户端地址。** 预检在这里回答，永不转发；客户端地址按可信代理规则解析。
2. **挂载点**——语法见下。
3. **OAuth issuer**，在 `{mount}/v1/oauth/…`，用 RFC 自己的错误信封。
4. **渠道声明的厂商服务路由**（Codex 的 `/backend-api/…`、Claude Code 的
   `/api/…`）。只在 Provider 或命名空间挂载上：匹配一条服务路由需要渠道，而聚合
   挂载不指名任何渠道。视图用 `x-gproxy-view: caller | pool | credential:{id}` 选。
   渠道声明为 socket 的那条路由走升级，不走调用。
5. **数据面**——`/v1/messages`、`/v1/responses`、`/v1/chat/completions`、
   `/v1beta/models/{model}:generateContent`，以及 files、images、audio、video、
   realtime 各面。先解码体，再匹配操作，然后 `App::call`；若命中的是握手面，
   则是 `App::connect`，随后升级。
6. **静态控制台**，带 SPA 回退，受 `console.enabled` 控制。

走到最后仍未命中的一律 404。

### WebSocket

| 路由 | 操作 / 方言 | 是什么 |
|---|---|---|
| `GET {mount}/v1/realtime` | `ConnectRealtime` / OpenAI | realtime 会话，可用 `?call_id=` 续接一次通话 |
| `GET {mount}/v1/live` | `ConnectRealtime` / OpenAI | 同上，WebRTC 的那种写法 |
| `GET {mount}/v1/live/{call_id}` | `ConnectRealtime` / OpenAI | 通话 id 写在路径里的续接 |
| `GET {mount}/v1/responses/ws` | `GenerateContent` / OpenAI Responses-over-WS | Responses 的 websocket 信封 |
| `GET {mount}/ws/v1beta/BidiGenerateContent` | `ConnectRealtime` / Gemini | Gemini Live |
| `GET {mount}/backend-api/…` | 渠道的 `ServiceRoute`，`ServiceTransport::WebSocket` | Codex 的远程控制服务端 |

三条 OpenAI realtime 路径正好是 `Core::connect_realtime_path` 会派发的那三条
路由，有一个测试断言两张表一致。`POST /v1/realtime/calls` **不在**其中：SDP
offer 是一个 HTTP multipart 请求，而续接它的握手根本不带体（从这里一路到
`Upstream::connect` 都是 `WireRequest<()>`）。真正跨越升级活下来的是通话
id——在路径里或在 `?call_id=` 里——core 用它把这条 socket 钉在当初回答 offer 的
那张凭证上。查询串原样转发，正是为了这个。

**在获准之前什么都不会被升级。** 先认证，再看握手形状，再准入，再打上游握手，
`101` 最后才写。所以每一次拒绝都是客户端读得到的 HTTP 回答——一条被接受又立刻
关掉的 socket 既没有状态码、也没有体、也没有 code。

**上游拒绝的握手原样转达。** 上游的状态、头和体照原样到客户端；厂商自己的
`429 {"error":{"code":"insufficient_quota"}}` 比本网关能编出来的任何 502 都值钱。
这个体是本宿主唯一有意缓冲的响应：处在握手中途的客户端是从它的握手缓冲里读剩下
那段响应的，而分块转发到它手里时 chunk 框架还在里面。

**socket 开着多久就占着租约多久。** pump 持有与流式响应体同一个 `Trailer`——
`Admitted`、下游 capture、core 的 `UsageCompletion`——并在 socket 关闭时释放，
而不是在写完 `101` 时。跑一小时的会话就占一小时的并发额。

pump 双向转发 text 与 binary；ping/pong 留在它来的那一侧，因为心跳量的是它走过
的那条链路。一侧的 close 会转给另一侧，然后把关闭握手读到底（有五秒宽限），这正
是让 core 把这次交换结算为「完成」而不是「中断」的原因。超过 `max_ws_frame_bytes`
的帧会用 `1009 Message Too Big` 关掉两边。

**记什么。** 每条 socket 一行 `capture_records`，kind 是 `ws_connection`，状态
`101`；再加每条消息一行 `capture_events`——text、binary、ping、pong、close，跨两
个方向统一排序，各带一个 `direction`。帧和别的体一样受 `enable_downstream_log_body`
控制、按同一套规则脱敏；关掉它时连接仍然记录，但调用者说过的话一个字都不复制。
累计 64 KiB 后停止记录，行的 body state 写 `partial`。

`turn_id` 恒为空，也不写 `ws_turn` 行。schema 把它说成「可识别时的 WS 业务轮次」，
而「轮次」是某个方言的概念——OpenAI 的 `response.created`/`response.done`、
Gemini Live 自己的一套——本宿主则是把 realtime 帧当不透明字节转发（除
`OpenAiResponsesWebSocket` 外，core 拒绝转换任何 websocket 方言）。在这里切轮次
就是替线上协议发明一条它没画过的边界。

厂商服务 socket 什么都不占、什么都不记，这是 `gproxy-app` 的规矩而不是本 crate 的：
服务跑在观测漏斗之外。它能代表哪张凭证说话是渠道说了算——Codex 直接拒绝合成视图，
所以只有该凭证的管理员带 `x-gproxy-view: credential:{id}` 才过得去。

#### OAuth 端点

相对于挂载前缀（`""`、`/acme`、`/openai-prod`）：

```
GET  {prefix}/v1/oauth/authorize    同意页交接（浏览器会被 302 到门户）
POST {prefix}/v1/oauth/authorize    本人的决定
POST {prefix}/v1/oauth/token        code / refresh / device 三种 grant
POST {prefix}/v1/oauth/device/code
POST {prefix}/v1/oauth/revoke
GET  {prefix}/v1/.well-known/oauth-authorization-server
GET  /.well-known/oauth-authorization-server{prefix}/v1   （RFC 8414 §3.1）
```

issuer 标识是 `{origin}{prefix}/v1`。P7 的模块注释里把聚合挂载写成
`https://host`，这里改正为 `https://host/v1`：这样一条规则就能生出三个挂载，而
`Issuer::metadata` 的 `{issuer}/oauth/…` 在每个挂载上都正好落在上表的路径上。

## 这层不做的判断

凡是下层已经给过答案的问题，这里一律不再回答一遍：

| 问题 | 由谁回答 |
|---|---|
| 调用者是谁 | `gproxy_app::Authenticator` |
| 他能够到什么 | `gproxy_app::Admission` |
| 哪个 Provider 服务这个模型 | sdk 的 resolver |
| 一个失败在协议上值多少 | `AppError::status_code` / `code` |
| 一个操作做什么 | `gproxy_app::Operations` / `Portal` / `Issuer` |

它负责的是：路由、头与体的解码、CORS、客户端地址、挂载语法、两种错误信封、
流式转发、WebSocket 升级与双工 pump，以及限流租约的生命周期。

## 挂载语法

同一个实例在三个挂载上提供同一套上游 API：

| 路径 | 挂载 | 收窄到 |
|---|---|---|
| `/v1/messages` | 聚合 | 不收窄 |
| `/acme/v1/messages` | 命名空间 `acme` | `acme/` 下的暴露名 |
| `/openai-prod/v1/messages` | Provider `openai-prod` | 该 Provider |

**命名空间**是带斜杠的暴露模型名的第一段（暴露 `acme/fast` 就产生命名空间
`acme`）。**Provider** 用的是 `providers.name`，即运维给的标签，而不是行 id——
没人会把机器铸的 id 写进客户端的 base URL。同名时命名空间优先。

**只有当剩下的部分同样是一个已声明的 ingress 面时，前缀才会被剥掉。** 这正是这套
语法存在的理由，任何简化都是错的：否则一个叫 `backend-api` 的 Provider 会把
`/backend-api/codex/responses` 吃掉——那是 Codex 的真实路径——把一次数据面调用变成
一个并不存在的挂载。

**因此有歧义的路径一律倒向聚合挂载。** 当第一段确实指到了什么、但剩下的部分不是
本网关提供的面时，路径原样保留。丢掉一个挂载是运维看得见的 404；取错一个挂载是把
请求发给了错误的上游。

挂载通过**给模型名加前缀**来收窄选路：`/acme/v1/messages` 配
`{"model":"fast"}` 解析的是 `acme/fast`。这正是 sdk resolver 自己的
`provider/model` 语法，宿主没有新增规则。已经带着前缀的名字原样保留，所以客户端
两种写法都行。

## 可信代理规则

`x-forwarded-for` 与 `x-forwarded-proto` 是头，而头是对端想写什么就写什么。
**只有当 socket 的对端地址是回环、或在 `trusted_proxies` 里时**才采信；来自其他
对端时直接忽略——不合并、不优先、也不作为兜底——而默认配置谁都不信。

两者的代价不同，但都要紧：伪造的 `x-forwarded-for` 决定了运维日志里记在某人请求旁
的地址；伪造的 `x-forwarded-proto` 决定了 **OAuth issuer 标识的 scheme**，而那是一
份告诉客户端"把授权码送到哪里"的发现文档。

服务端若没有用 `into_make_service_with_connect_info` 构建，对端就是未知的，而未知
对端一律按不可信处理。

## 租约生命周期规则

`CallOutcome::admitted` 持有本次请求的限流计费，而并发许可衡量的是**在途**请求。
流式响应在 `App::call` 返回之后很久仍然在途。

所以响应体是一层包装流（`response::LeasedBody`），它**拥有**那份准入决定：
`Admitted`、`DownstreamCapture` 和 core 的 `UsageCompletion` 都住在这个流里面。
每个 chunk 经过时喂给 capture；最后一个写完之后，流去 await 结算、落下 capture
记录与它的边，然后释放租约。没有谁需要"记得"释放什么，因为这些值没有别的存放处。
客户端中途断开则是把响应体 drop 掉：租约由 `Admitted` 自己的 drop 路径归还，
capture 丢失——这与 core 的 observer 付出的代价相同。

WebSocket 是同一条规则，只是钟走得更久。pump 持有与响应体相同的那个 `Trailer`，
socket 的租约在 socket 关闭时释放——`101` 不是任何东西的结束。await 结算之前会
先把 socket 的两半 drop 掉，因为 core 是把这次交换的结算装进它交出来的那条
socket 的：结算它的 guard 就住在 incoming 流里，所以一边攥着 socket 一边 await
`UsageCompletion`，等的是一个只有 drop 它才会产生的值。

## 两种错误信封

产品面回答

```json
{ "error": { "code": "forbidden", "message": "…" } }
```

OAuth 端点回答 RFC 6749 §5.2 的

```json
{ "error": "invalid_grant", "error_description": "…" }
```

它们是两个各自独立的 `IntoResponse` 类型，端点靠"返回哪个包装"来选信封，因此在调用
处不可能混淆。OAuth 客户端读不懂前一份文档：RFC 规定 `error` 是字符串，客户端在那
里读到一个对象就等于没读到 code，分不清"重新登录"和"继续轮询"。

5xx 的消息永远不回给调用者——它可能引用 DSN、某一行数据或某个头。code 会回，
文本只进运维日志。

## 已知限制

- **Provider 挂载上不带模型的操作只收窄到该 Provider 的渠道，而不是该 Provider。**
  `DataPlaneRequest` 有 `channel` 字段而没有 provider 字段，所以
  `GET /p1/v1/models` 会列出调用者能够到的、`p1` 所在渠道上所有 Provider 的模型。
  要收干净得给 `DataPlaneRequest` 加一个字段。
- **WebSocket：有意没做的部分。** 本宿主自己不做子协议协商——上游选了什么就原样
  回给客户端，此外不提供任何选项。帧原样转发，core 对帧应用的改写规则是 core 的。
  socket 在这里没有空闲超时，因为 core 自己就写了：realtime 会话「由取消和帧上限
  约束，而不是由 HTTP 的空闲/总超时约束：一个 realtime 会话理应在等人说话」。
  也没有按轮次的 capture，理由见上。
- **登录没有限流。** `gproxy-app` 说明了它为什么做不了（它没有客户端地址）；本宿主
  有，但还没用上。
- **客户端断开不会取消上游调用。** `DataPlaneRequest::cancellation` 留空。
  WebSocket 天生是例外：客户端走了 pump 就结束，上游 socket 随之关闭。

## 测试

`cargo test -p gproxy-host-axum`。每个集成测试都在内存实例上构建真实的 router；
只有上游是脚本化的。直接调用 handler 函数的测试会跳过真正要测的那一层。

绝大多数用例用 `tower::ServiceExt::oneshot` 驱动 router。WebSocket 的往返不行：
`oneshot` 永远不会产生 hyper 的 `OnUpgrade` 扩展，而升级就是由它构成的，所以
`tests/websocket.rs` 绑一个本机端口，用 `tokio-tungstenite` 真说协议。每一种拒绝
仍然是 `oneshot` 用例——这本身就是「它没有升级任何东西」的断言。
