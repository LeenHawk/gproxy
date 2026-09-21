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
5. **数据面**——`/v1/messages`、`/v1/responses`、`/v1/chat/completions`、
   `/v1beta/models/{model}:generateContent`，以及 files、images、audio、video、
   realtime 各面。先解码体，再匹配操作，然后 `App::call`。
6. **静态控制台**，带 SPA 回退，受 `console.enabled` 控制。

走到最后仍未命中的一律 404。

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
流式转发，以及限流租约的生命周期。

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

- **引擎的执行 future 不是 `Send` 的。**
  `gproxy_core::execute::attempt` 与 `gproxy_core::service` 跨 await 持有
  `&WireRequest` / `&ServiceRequest`，而这两个类型带着 `HttpBody`，它的流式变体是
  `Send` 但不是 `Sync`。axum handler 的 future 必须是 `Send`，所以每次引擎调用都
  过一遍 `on_engine_thread`：在一个 blocking 线程上创建并驱动这个 future。它是正确
  的——`Handle::block_on` 进入主 runtime，所以连接与定时器仍归主 runtime——但每个
  在途调用都会占住一个线程直到响应头到达，这是本网关不该留着的并发硬上限。
  **修法在 `gproxy-core`**：别再跨 await 借用 request，改完之后这里每个调用点都变
  回普通 `.await`，这个辅助函数就没了。P9 之前没人发现，是因为既有测试都跑在
  `#[tokio::test]` 的 current-thread runtime 上，那里没有 `Send` 约束。
- **Provider 挂载上不带模型的操作只收窄到该 Provider 的渠道，而不是该 Provider。**
  `DataPlaneRequest` 有 `channel` 字段而没有 provider 字段，所以
  `GET /p1/v1/models` 会列出调用者能够到的、`p1` 所在渠道上所有 Provider 的模型。
  要收干净得给 `DataPlaneRequest` 加一个字段。
- **WebSocket 升级回 `426`。** 路由已经声明，好让挂载语法先认得它们，客户端拿到的
  是一句明确的拒绝，而不是一个看起来像"模型不存在"的 404；升级本身由 P10 实现。
- **登录没有限流。** `gproxy-app` 说明了它为什么做不了（它没有客户端地址）；本宿主
  有，但还没用上。
- **客户端断开不会取消上游调用。** `DataPlaneRequest::cancellation` 留空。

## 测试

`cargo test -p gproxy-host-axum`。每个集成测试都在内存实例上构建真实的 router，
并用 `tower::ServiceExt::oneshot` 驱动它；只有上游是脚本化的。直接调用 handler
函数的测试会跳过真正要测的那一层。
