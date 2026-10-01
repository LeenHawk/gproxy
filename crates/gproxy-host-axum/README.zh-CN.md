# gproxy-host-axum

[English](README.md) | 简体中文

GPROXY v4 的原生 HTTP 宿主：架在 [`gproxy-app`](../gproxy-app) 之上的一层
[axum](https://docs.rs/axum) 路由。它是一层**传输绑定**——把字节变成
`gproxy-app` 暴露的带类型调用，再把结果变回字节，自己不含任何产品行为。

能编到原生目标，**也能编到 `wasm32-unknown-unknown`**，而且两边是同一张 `Router`：
[`gproxy-host-edge`](../gproxy-host-edge) 把它挂进 Cloudflare Workers 的 fetch
处理器，而不是把路由表再写一遍。wasm 构建里少掉的都是"插座形状"的东西，从来不是路由
——内嵌 console（`rust-embed` 和文件系统）、websocket 升级（hyper 的 `OnUpgrade`）、
以及 `peer_ip` 用的 `ConnectInfo`（axum 的 `tokio` feature，在那边换成
`cf-connecting-ip`）。

wasm 目标对 handler 的唯一要求是函数体外面包一层 `crate::send`。wasm 上引擎按设计
就是 `!Send`——JS 传输句柄属于创建它的 isolate——而 axum 的 `Handler` 要求
`Future: Send`。`send` 在原生是恒等函数，在那边是 `send_wrapper::SendWrapper`，
所以漏包的 handler 会让 wasm 构建报一条点名到它的错，而不是错在别处、或者在边缘
悄无声息地不存在。完整结论在那个 crate 的 README 里。

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
| — | `/admin/api/…` | 管理面：每个操作一条显式 `MethodRouter`，覆盖 `Operations`、`manage()` 与 `ScopedManage`；调用者的 `AdminScope` 决定他看得见多少 |
| — | `/portal/api/…` | 终端用户面，外加 `login` 与 `logout` |
| — | 其余一切 | ingress 兜底，顺序见下 |

### `/admin/api`

两半，一个面。**身份**家族来自 `gproxy_app::Operations`；**配置**家族直接调
`gproxy_sdk::Gproxy::manage()`——sdk 的家族本身就是那张操作表，再包一层转发也
没有任何东西可判断。例外是 `credentials` 与 `quotas`：它们的行带 owner，走
`gproxy_app::ScopedManage`——那一层确实有东西要判断。两半共用同一组中间件。

#### scope

这个面不再只属于实例管理员。每个请求带一个 `gproxy_app::AdminScope`——
`Instance`、`Organization(id)` 或 `Team(id)`——组织管理员调**同样的路由**，
区别只是回来的行更少。

| 调用者 | scope |
|---|---|
| `users.role = admin` | `Instance`；不读 scope 头 |
| API key（含 OAuth 授权的内部 key） | key 自己的 `team_id`，否则 `organization_id`——永远不看请求头 |
| 控制台会话 | `x-gproxy-admin-scope` 头，按调用者的 **admin** 成员关系校验 |

`x-gproxy-admin-scope` 取 `instance`、`organization:{id}`（或 `org:{id}`）、
`team:{id}`。只管理一个 scope 的会话不需要这个头；管理多个的必须指定，在指定之前
每条路由都回 `400` 并点名这个头；一个都不管理的整面 `403`，和没有 scope 之前一样。

scope **参与构造查询**，而不是拿来核对答案——这正是它安全的原因：

- 列表在语句构造之前经 `AdminScope::narrow` 收窄，数据库永远不会执行一条可能命中
  他人行的查询。筛选条件指向 scope 之外的 owner 返回**空页**，不是错误；
- 按 id 取行的路由先读出该行的 owner，越界时回 **`NotFound`，绝不是 `Forbidden`**
  ——`Forbidden` 会确认这个 id 存在，而那正是枚举想要的事实；
- 写入时点名 scope 之外的 owner 是 **`Forbidden`**，因为这个 id 是调用者自己填的，
  不是发现的。patch 校验两次——按当前的行和按改完的行——所以一行既不会被推出 scope，
  也不会被拉进来。

哪些家族对哪些 scope 开放，只声明**一次**：
`gproxy_app::admin_surface::ADMIN_SECTIONS`。下面每个路由宏都把 section id 作为
**必填**参数——什么都不声明的家族编译不过——生成的 handler 再调 `require_section`，
而它对不认识的 section 一律拒绝。这张表因此默认是关的。

目前开放的 section 是 `context`、`session`、`credentials`、`quotas`。其余都是实例
机器：网关自身的配置（Provider、模型、路由、改写、端点、价格、连接档、设置、
导入导出、连通性、词表、目录），以及——暂时——身份家族。按组织收窄身份是下一步，
刻意不做成半开。

`quotas` 一张表同时装着这条分界的两边：owner 是 `org`/`team` 的是租户预算，owner 是
`credential`/`provider` 的是运维限额。分界在 `owner_kind` 上，不在路由上，所以
`/quotas/{id}/limit-reset` 和别的按 id 取行的路由一样挂着，在实例 scope 之外直接
`NotFound`。

#### `GET /admin/api/context`

唯一一条任何已认证调用者都够得着、与 scope 无关的路由，包括还没定下 scope 的人。
**控制台的导航只从这里渲染，别无他处。**

```json
{
  "user": { "id": "…", "name": "orgadmin", "role": "user", "instanceAdmin": false },
  "callerKind": "session",
  "scopeHeader": "x-gproxy-admin-scope",
  "scope": { "kind": "organization", "id": "…", "name": "acme",
             "organizationId": "…", "selector": "organization:…", "current": true },
  "scopes": [ … 这个调用者可以充当的每一个 scope … ],
  "sections": [ { "id": "credentials", "path": "/credentials",
                  "capabilities": ["read", "write"] }, … ]
}
```

`selector` 就是选中该 scope 要发的头的字面值，控制台不必自己拼。只有当调用者管理
多个 scope 且一个都没指定时，`scope` 才是 `null`（`sections` 随之为空）。
`/admin/api/session` 和它挂在同一个较轻的 guard 下，所以在选定 scope 之前、以及
scope 失效之后，登出都还能用。

#### 身份

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
GET    /admin/api/usage                        全局汇总、分组与趋势（仅实例管理员）
GET    /admin/api/usage/records                用量明细分页
GET    /admin/api/logs/downstream              下游请求，游标分页
GET    /admin/api/logs/upstream                上游物理请求，游标分页
GET    /admin/api/logs/downstream/{id}         下游请求、关联上游与用量详情
GET    /admin/api/logs/captures/{id}           单次采集与流式事件
```

#### 配置

同样的五条，外加第六条——每个 sdk 家族都有 batch，而一次 batch 不论写多少行
都只是一次 revision 提交：

```
POST   /admin/api/{family}/batch     [{"create": …}, {"update": {"id": …, "patch": …}}, {"delete": "id"}]
```

`{family}` 取 `providers`、`credentials`、`models`、`provider-models`、
`routes`、`route-members`、`connection-profiles`、
`rule-sets`、`rules`、`provider-rule-sets`、`operation-rules`、
`operation-endpoints`、`quotas`、`price-rules`、`price-rates`、`price-tiers`。
之外：

```
GET    /admin/api/settings                       唯一那行的两组设置
PATCH  /admin/api/settings
POST   /admin/api/providers/{id}/routing-defaults/reset
POST   /admin/api/credentials/{id}/reveal        明文密钥——会审计
POST   /admin/api/credentials/{id}/status        {"status": "active"|"dead", "reason": …}
POST   /admin/api/credentials/{id}/refresh       ?force=true 连没过期的也换
GET    /admin/api/credentials/{id}/quota         周期（进行中 + 最近关闭的）与封禁
GET    /admin/api/credentials/{id}/quota-observations  原始观测，分页，?sinceMs&untilMs
POST   /admin/api/credentials/{id}/quota-probe   去问上游
POST   /admin/api/credentials/{id}/quota-reset   兑换一次重置额度
POST   /admin/api/credentials/{id}/health-reset
GET    /admin/api/credentials/{id}/limits        覆盖它的运维限额
POST   /admin/api/models/discover                {"providerId": …, "credentialId": …}
POST   /admin/api/models/discover/apply          {"providerId": …, "upstreamNames": […]}
POST   /admin/api/models/test                    真打一次生成
PUT    /admin/api/rule-sets/{id}/rules           整组替换
POST   /admin/api/rule-sets/{id}/rule-presets/{preset}
GET    /admin/api/quotas/status                  ?owners=user:alice,team:t1
POST   /admin/api/quotas/{id}/reset              预算窗口
POST   /admin/api/quotas/{id}/limit-reset        运维限额造成的封禁
POST   /admin/api/export                         {"includeSecrets": bool} → no-store
POST   /admin/api/import                         {"export": …, "mode": …, "sourceMasterKey": …}
POST   /admin/api/connectivity/test
GET    /admin/api/channels                       每个编译进来的渠道一份 ChannelDescriptor
GET    /admin/api/tls-presets
GET    /admin/api/rule-presets
GET    /admin/api/default-model-catalog
POST   /admin/api/default-model-catalog/apply-prices
GET    /admin/api/tokenizer-vocabs               （及 POST 拉取一份）
GET    /admin/api/tokenizer-vocabs/progress      本进程的下载进度，或 null
DELETE /admin/api/tokenizer-vocabs/{fileId}
GET    /admin/api/tokenizer-auth                 （及 PATCH {"token": … | null}）
POST   /admin/api/tokenizer-auth/reveal          明文 token——会审计
```

v3 有同一个操作的地方路径沿用 v3，运维已有的脚本因此不会断。v4 新增的：
`/connection-profiles`、`/operation-rules`、
`/operation-endpoints`、`/price-tiers`、`/settings`（v3 分成
`/instance-settings` 与 `/log-settings`）、`/{family}/batch`（v3 是
`/batch/{entity}`）、`/credentials/{id}/{status,refresh,limits}`、
`/models/discover/apply`、`/rule-sets/{id}/rules`、`/quotas/status`、
`/quotas/{id}/{reset,limit-reset}`，以及
`DELETE /tokenizer-vocabs/{fileId}`（v3 把 id 放在 `DELETE` 的 body 里）。

#### 中间件

中间件顺序：认证 → **解析 scope**（`AdminScope::resolve`，派生 scope 的唯一一处，
也是读 scope 头的唯一一处）→ 非安全方法且是 cookie 调用者时校验同源 →
执行操作，按它声明的 section 把关 → 所有非渠道 API 操作（包括 GET 查询）写一行审计。它是 `route_layer`，所以
未命中的 `/admin/api/*` 是一个连数据库都不碰的 404。

`/admin/api/context` 与 `/admin/api/session` 挂在第二个、更轻的 guard 下：解析完就
停，不要求已经定下 scope。

审计的 action 由命中的路由推出（`admin.api_keys.rotate`、
`admin.providers.create`），新路由因此不可能忘记给自己命名。查询和写入都审计。渠道模型／服务请求进入下游日志，服务不扣用量。拒绝也会审计：组织管理员操作被拒的 `403`
会以 `outcome = error` 落在审计里。

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
GET    /portal/api/oauth/device      查询待批准的设备码（?userCode=）
POST   /portal/api/oauth/device      批准或拒绝；需要登录会话或带管理权限的 key
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
| `GET {mount}/v1/responses` | `GenerateContent` / OpenAI Responses-over-WS | Responses websocket 会话 |
| `GET {mount}/ws/v1beta/BidiGenerateContent` | `ConnectRealtime` / Gemini | Gemini Live |
| `GET {mount}/backend-api/…` | 渠道的 `ServiceRoute`，`ServiceTransport::WebSocket` | Codex 的远程控制服务端 |

Responses 在升级前认证，再根据每条 `response.create` 的模型执行准入和选路。
不同 lane 可以并发，续接链保持目标绑定并复用原生连接。HTTP 桥接通过后续生成段支持
上下文预热、中断、steering 和 injection。生成并发与用量按轮次计算，空闲连接不占生成
名额；连接事件可关联 `WsTurn`，不重复保存消息体。

下面的握手与租约规则针对 realtime 会话。
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
开启正文采集后完整记录收到的帧，不按日志大小截断。

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
GET  {prefix}/v1/oauth/authorize    同意页交接（浏览器会被 302 到 `/console/authorize`）
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
`Admitted`、`DownstreamCapture`、core 的 `UsageCompletion`，以及本次请求的取消
守卫，都住在这个流里面。每个 chunk 经过时喂给 capture；最后一个写完之后，流去
await 结算、落下 capture 记录与它的边，然后释放租约。没有谁需要"记得"释放什么，
因为这些值没有别的存放处。客户端中途断开则是把响应体 drop 掉：租约由 `Admitted`
自己的 drop 路径归还，capture 丢失——这与 core 的 observer 付出的代价相同。

WebSocket 是同一条规则，只是钟走得更久。pump 持有与响应体相同的那个 `Trailer`，
socket 的租约在 socket 关闭时释放——`101` 不是任何东西的结束。await 结算之前会
先把 socket 的两半 drop 掉，因为 core 是把这次交换的结算装进它交出来的那条
socket 的：结算它的 guard 就住在 incoming 流里，所以一边攥着 socket 一边 await
`UsageCompletion`，等的是一个只有 drop 它才会产生的值。

## 客户端走了就取消上游调用

**一个请求一个 token。** ingress handler 造一个 `response::CancelOnDrop`，把它的
token 克隆一份放到 `DataPlaneRequest::cancellation` 上，自己留着守卫。core 处处
认这个 token：每次尝试之前、发送途中、流式途中，以及——对运维真正要紧的那处——
决定 usage 行该写什么的时候。

客户端从不宣告自己要走；它永远是一次 **drop**，而且可能发生在两个位置。所以守卫
由每个位置上还活着的那个东西持有：

- **响应头还没写出去**时，handler future 被 drop，守卫就在它的栈上。
- **流到一半**时，hyper drop 响应体，而此时守卫已经搬进 `LeasedBody` 里的
  `Trailer`——和租约、capture、结算放在一起，理由与它们为什么住在一处相同。
- **socket 上**由 pump 显式取消，因为升级后的 socket 是 hyper 另起的任务在泵，
  handler future 早就没了，没有什么可供 drop。凡是意味着*客户端走了*的出口都取消：
  它的流结束、它的流出错（被 drop 的 socket 到我们这里是
  `ResetWithoutClosingHandshake`，那是一次失败而不是一次结束）、以及写给它却写不
  出去的一次发送。取消发生在关闭帧已经送往上游之后、排空关闭握手之前，而那次读
  正是让 core 看见取消的地方。

**正常结束绝不取消。** core 在 `Completed` 与 `Cancelled` 之间做选择时会读这个
token，所以最后一个字节之后才触发的 token 会往 usage 行里写进一句假话，并且会和
响应体正在等的那次结算抢跑。因此守卫只在一个地方解除——`Trailer::finish`，所有正常
结束都从那里经过：响应体读完、上游中途失败、握手被拒、socket 关闭。任一侧发来的
关闭帧都是一场谈完的会话，不是取消。

被取消的请求仍然是被计量的请求。它和别的请求一样有 usage 行，只是 `metrics.state`
写的是 `cancelled` 而不是 `completed`，带着上游在被叫停之前来得及报出来的那些数，
capture 记录也按 cancelled 收尾。租约照旧归还。

厂商服务调用与模型请求共用同一套取消机制，覆盖凭据准备、上游调用和 HTTP 响应体读取。
WebSocket 服务的 token 覆盖握手阶段，已建立的连接由宿主管理。服务仍不占限流租约，
也不产生模型用量。

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

- **WebSocket：有意没做的部分。** 本宿主自己不做子协议协商——上游选了什么就原样
  回给客户端，此外不提供任何选项。帧原样转发，core 对帧应用的改写规则是 core 的。
  socket 在这里没有空闲超时，因为 core 自己就写了：realtime 会话「由取消和帧上限
  约束，而不是由 HTTP 的空闲/总超时约束：一个 realtime 会话理应在等人说话」。
  也没有按轮次的 capture，理由见上。
- **身份家族还没有 scope 化。** 组织管理员能到 `credentials` 与 `quotas`；用户、
  key、团队、成员、权限、限流、订阅、池、套餐、OAuth 客户端、会话和审计仍然是
  `Instance`-only。机制已经就位——改 `ADMIN_SECTIONS` 里的一行、再给那个家族一个
  收窄规则就开——但每个家族都是一次有意的动作而不是一个开关，因为其中多数要的是
  对成员关系做 `IN`，而不是一次列比较。

## 测试

`cargo test -p gproxy-host-axum`。每个集成测试都在内存实例上构建真实的 router；
只有上游是脚本化的。直接调用 handler 函数的测试会跳过真正要测的那一层。

绝大多数用例用 `tower::ServiceExt::oneshot` 驱动 router。WebSocket 的往返不行：
`oneshot` 永远不会产生 hyper 的 `OnUpgrade` 扩展，而升级就是由它构成的，所以
`tests/websocket.rs` 绑一个本机端口，用 `tokio-tungstenite` 真说协议。每一种拒绝
仍然是 `oneshot` 用例——这本身就是「它没有升级任何东西」的断言。

`tests/cancel.rs` 绑端口是出于第二个理由：断开没法伪造。树里每个 HTTP 客户端在把
响应交出来之前都会先把体读干净，所以那一组用例直接在裸 `TcpStream` 上把请求敲出来，
再靠 drop 它来挂断——那是真实客户端会做、而这里没有别的东西能模仿的唯一一件事。

`tests/scope.rs` 是一张真值表而不是几个轶事：四种 scope（实例、组织、团队、无）
乘以每一类家族的一个代表，再加上不属于表里任何一格的那几条规则——scope 之外的行是
`NotFound`、写入点名他人 owner 是 `Forbidden`、头里点名没管理的 scope 被拒、
API key 的绑定压过它自己发的任何头。
