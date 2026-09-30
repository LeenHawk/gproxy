# gproxy-app

[English](README.md) | 简体中文

GPROXY v4 的产品层：谁在调用、允许他做什么，以及改变这两者的操作。它拥有身份
（用户、网关 API key、组织、团队、权限、限流、OAuth issuer、审计）、准入，
以及带类型的管理面／门户／issuer 操作。

它不拥有任何 server。这里没有 HTTP 框架、没有路由器、没有运行时、没有 CLI，也没有
引擎的任何部分：选路、凭证选择、失败转移、协议转换、结算与捕获都属于 `gproxy-core`
和它之上的句柄。正是这条界线让同一套判断既能跑在原生 axum server 后面，也能跑在
Cloudflare Worker 里——本 crate 能为 `wasm32-unknown-unknown` 构建，而且会一直如此，
因为它本来就没有任何与平台绑定的东西。

`App<C>` 在**每个**目标上都是 `Send + Sync`，这样宿主才能把它放进请求级 state——
`axum::Router<S>` 要 `S: Clone + Send + Sync + 'static` 才肯接。这在 wasm 上不是白来的：
那边的引擎按设计就是 `!Send`，因为 JS 传输句柄属于创建它的 isolate，`gproxy-client`
的 `ClientBounds` 在类型上写明了这件事。所以 `App` 在该目标上用
`send_wrapper::SendWrapper` 持有句柄，其余一律不变——一层包装，运行时检查而非断言，
因 Worker isolate 单线程而成立。`App` 自身方法返回的 future 在 wasm 上仍是 `!Send`；
宿主在各自的 handler 处桥接（见 `gproxy_host_axum::send`）。

## 一个请求的形状

```text
传输层（宿主）
  → auth        Caller   { 用户、角色、api key、组织、团队、grant }
  → admission   Admitted { 允许的 Provider 集、允许的凭证集、预算 owner 链、
                           scope、会话身份、限流许可 }
  → App::call   引擎只按这些执行，下游不再重新推导任何一项
```

每一步都在收窄，下面不会重新打开上面已经收窄的东西：交给引擎的是一个 Provider 集和
一个凭证集，而不是一个待解释的调用方。

`App` 是三样东西汇合的地方——引擎句柄、实例配置、身份快照——也是宿主唯一需要持有的值：

```rust,ignore
let app = App::new(gproxy, config);
app.reload_all().await?;                       // 启动时一次，两个快照一起

let data = app.data();                         // 每个请求取一次，全程持有
let caller = app.authenticator(&data).authenticate_request(&headers).await?;
let outcome = app.call(&caller, request).await?;
//   outcome.execution  → 要回写给客户端的响应
//   outcome.admitted   → 扣减；在流写完之前不能丢
```

`authenticator` 与 `admission` 接收快照而不是自己去取，因为它们会在整个生命周期里借用
它，而已发布的快照随时可能被换掉；这同时也把「一个请求只取一次快照」这条规则写进了
签名里。

## 现在有什么

| 模块 | 内容 |
|---|---|
| `config` | `AppConfig`：纯 serde，两个宿主共用，不碰文件系统也不读环境变量 |
| `snapshot` | `AppData`——某个配置 revision 编译后的身份，以及单调发布它的 `AppSnapshot` |
| `auth` | 请求指认调用方的三条路径，以及三者产出的同一个 `Caller` |
| `admission` | `Caller` → `Admitted`：Provider 集、凭证集、预算 owner 链、scope、会话、限流扣减 |
| `call` | `DataPlaneRequest` → `CallOutcome`：先准入，再让引擎严格按准入结果执行 |
| `service` | 厂商 CLI 服务：目标集里有哪些凭证，以及调用方对这组凭证是什么角色 |
| `capture` | 下游 `capture_records` 行，以及连向 core 所记上游尝试的 `capture_links` 边 |
| `publication` | `AppPublicationUrl`，以及宿主下载路由背后的读取与删除 |
| `operations` | 身份写入家族，每次写入一个 revision commit 加一次对等实例通知 |
| `operations::scoped` | `ScopedManage`：两个行带 owner 的 sdk 家族，按调用方的 `AdminScope` 收窄 |
| `admin_scope` | `AdminScope`：一次管理请求可以充当什么、它从哪里来、它放行什么 |
| `admin_surface` | `ADMIN_SECTIONS`：把管理面写成一张表，以及每个 section 的最小 scope |
| `operations::portal` | 终端用户的自助面，由构造本身锁定在一个 `Caller` 上 |
| `operations::issuer` | 本实例**面向下游客户端**运行的 OAuth 授权服务器 |
| `dto` | 这些家族交换的线上形状：String id、毫秒时间戳、camelCase，不含任何密文 |
| `audit` | 只追加的审计流水，写在 revision batch 之外，附带脱敏规则 |
| `error` | `AppError`，带 `status_code()` 与用于 API 信封的稳定 `code()` |

表里的模块都已实现。本 crate 还缺的是宿主：这里没有任何东西绑定传输层，那是
`gproxy-host-axum` 与 `gproxy-host-edge` 的事。

## 快照

`AppData` 之于本 crate，等同于 `CoreData` 之于 `gproxy-core`，而且两者刻意来自同一次
读取：`Store::load_all_data()` 在一个 batch 里返回 control、routing、identity 三组行，
因此引擎与产品层不会在一个请求中途差出一个 revision。一个请求只取一次
`AppSnapshot::load()`，并在整个生命周期里持有那个 `Arc`。

| 索引 | 回答 |
|---|---|
| `ApiKeyIndex` | 摘要 → `ApiKeyIdentity`（key、用户、角色、组织、团队、kind、过期） |
| `MembershipIndex` | 用户 → 带角色的组织与团队，外加每个团队的父组织 |
| `CredentialOwnership` | 凭证 → `Shared` / `User` / `Team` / `Org`，以及调用方能否选中它 |
| `PermissionSet` | `(主体, Provider, 模型, 操作)` → `Allow` / `Deny(原因)`，以及允许的 Provider 集 |
| `ClientAllowlist` | 某用户是否可以授权某个 OAuth client |

此外还以 map 形式带着 `users`、`organizations`、`teams`、`rate_limits`、
`oauth_clients`。

`publish_if_newer` 按 revision 单调，与 `Core::publish_snapshot` 是同一份契约：
重载会竞争——同一次写入既触发轮询又触发通知，先开始的慢重载可能后完成——而快照回退
会让已删除的 key 和已撤销的 grant 复活。

装配不会因为一行坏数据而失败。畸形的权限行、解不开的 key 摘要、同时有两个 owner 的
凭证都会被丢弃、计数并记日志；一行坏数据不该让一个正在正常服务的实例倒下。

### 本 crate 做出的、schema 没有规定的决定

- **`api_keys.key_hash` 是 key 文本 SHA-256 的小写十六进制。** 该列只是一个普通
  `String`，`gproxy-store` 并未规定编码，于是在这里规定：所有写入方与读取方都经过
  `snapshot::encode_key_hash` / `decode_key_hash`。从客户端出示的 key 要算哪几种摘要
  （原文，以及去掉 `sk-` 前缀后再算一次）是认证阶梯的事，不是索引的事。
- **同时设置了多个 owner 列的凭证按 user > team > org 解析。** 实体注释说管理层只会
  设置其中一个；设了多个的行本就畸形，取最窄的 owner 泄露最少。
- **权限判定取 `(priority DESC, id)` 顺序下第一条适用的规则。** 先遇到 deny 就拒绝，
  先遇到 allow 就放行，没有任何适用规则则拒绝。priority 正是用来在两个方向上从一条
  宽规则里切出例外的；在**同一** priority 上冲突的两条规则退化为按 id 排序，所以互相
  重叠的 allow/deny 规则应当使用不同的 priority。
- **同时写了 user 与 api key 的权限行是两者的交集**，绝不是并集。

### 刻意执行两次

OAuth client 允许列表在这里评估，同时也在 SQL 里评估。
`gproxy_store::operations::oauth_policy` 把同一个条件构造进执行授权的那条语句，
因此与授权并发的策略修改无法从「检查」和「插入」之间溜过去。`ClientAllowlist` 负责
快路径：尽早拒绝、列出 client、渲染门户，全都不需要一次往返。SQL 那份是权威，两者
必须一致。有一层这里加不上——全局允许列表在 `settings` 上，属于 `ControlData` 而不是
`IdentityData`。

## 认证

三种调用方，一个 `Caller`。这一层之上不会再重新推导谁在调用。

| 种类 | 凭证 | 绑定 |
|---|---|---|
| `ApiKey` | `Authorization: Bearer`、`x-api-key` 或 `x-goog-api-key` 里的网关 key | key 行上的组织与团队 |
| `OAuthGrant` | 已签发的 access token，经 `resolve_access_many` 解析 | grant 的内部 key 行，外加一个 `GrantContext` |
| `Session` | 由 `user_sessions` 支撑的控制台／门户 cookie | 没有：会话就是这个人本身 |

查询串里的 key（`?key=`，Gemini 的形状）刻意不从 header map 里读。查询串属于传输层；
接受这种形式的宿主自己取出来，再调用 `authenticate_token`。

### 摘要阶梯，以及为什么要去掉 `sk-`/`at-`

出示的 token 会按这个顺序用**两个**摘要查找：

1. `SHA-256(token)`——`generate_api_key` 写入的形式，即 v4 的 key 存的是整串
   `sk-…` 文本的摘要；
2. `SHA-256(载荷)`，其中载荷是去掉一个前导 `sk-` 或 `at-` 之后的 token——v3 写入的形式。

`sk-` 与 `at-` 是展示前缀，不是密钥的一部分。有些渠道拒绝不像 PAT 的 token，于是同一个
key 会以 `at-…` 交给一个上游、以 `sk-…` 交给另一个。对载荷做摘要让三种写法——`sk-X`、
`at-X`、裸 `X`——保持同一个身份，配额与用量因此跟着 key 走，而不是跟着拼写走。没有前缀的
token 只产生一个摘要，不是两个。

### 三条路径共同遵守的规则

- **OAuth key 绝不能直接认证。** `api_keys.kind = oauth` 标记的是某个 grant 的内部 key：
  它承载该 grant 的绑定与用量身份，把它的文本当作 bearer key 出示会被拒绝，而不是当成
  未命中；也不会因此落到 access token 那条路径上。
- **所有失败一律 401，绝不 403。** 被禁用的 key、过期的 key、已撤销的 grant 与根本不存在的
  key，从外面看必须无法区分；`Forbidden` 等于说"这个凭证是真的，只是没有权限"，而这正是
  探测者想确认的事实。`AppError::Unauthorized` 带着给运维日志用的原因，对外只呈现
  `unauthorized` 一个词。403 属于准入，在身份确定之后。
- **过期按请求时钟复查。** key 索引在装配时会排除已过期的行，但一个 key 完全可能在这份
  快照仍然生效的期间过期。
- **token 只以哈希形式存储。** `api_keys.key_hash`、`user_sessions.token_hash` 与
  `oauth_tokens.token_hash` 存的都是文本的 SHA-256。明文只在创建时返回一次，实例无法
  再次产出它。两个字符串列共用 `encode_key_hash`，因此这里只有一种编码。
- **grant 的有效性不在这里重新实现。** `resolve_access_many` 在读取 token 的同一条语句里
  检查 token、grant、client、用户、内部 key 与 client 允许列表，因此并发的撤销无法
  从「检查」和「使用」之间溜过去。

### CSRF 在哪里生效

`verify_same_origin` 是一个独立函数而不是阶梯中的一步，因为它**只对 cookie 认证的请求
生效**。cookie 会被浏览器附加到任何到达该 origin 的请求上，包括由外站页面引发的请求；
放在头里的 API key 不会，外站页面也读不到它。对 API key 调用方跑这个检查只会让所有非浏览器
客户端失效而毫无收益。宿主在认证之后、对 `Session` 调用方、在操作执行之前调用它。

安全方法（`GET`、`HEAD`、`OPTIONS`）一律通过。其余方法都要求一个与请求自身 `Host` 相符、
或位于 `cors_origins` 之中的 `Origin`。与 v3 不同，不安全方法上缺失 `Origin` 会被拒绝：
浏览器一定会发这个头，缺了就说明对面不是浏览器，把它当作可信等于留下一个任何攻击者都能
选择的豁免口。

### 密码

argon2id，每次哈希一份全新的 16 字节盐，存成完整的 PHC 串，因此日后调高参数不会让已有
密码全部失效。策略沿用 v3——密码不能为空——再加上最少 8 个字符、最多 1024 字节。除此之外
没有任何组合规则：强制字符类别只会把用户推向 `Password1!`，并且挡住长口令。解析不了的
`password_hash` 意味着这个用户无法登录，绝不是 panic，也绝不会意外通过。

## 准入

`Admission::admit` 接收一个 `Caller` 和一个请求，产出引擎将要据以执行的 `Admitted`。
六步，顺序固定：

| # | 步骤 | 产出 | 可能拒绝为 |
|---|---|---|---|
| 1 | 权限 | 允许的 Provider 集 | `403 forbidden` |
| 2 | 凭证可见性 | 允许的凭证集 | — |
| 3 | 预算链 | 这次请求记在谁头上 | — |
| 4 | scope | 它算作谁的流量（用于亲和） | — |
| 5 | 会话身份 | 它延续的是哪段对话 | — |
| 6 | 限流 | 它正持有的扣减 | `429 rate_limited` |

1–5 步都是快照与请求的纯函数。**第 6 步放在最后，因为只有它会消耗东西。**
因权限被拒的请求不能拨动共享计数器，否则未授权的客户端只要不断发送它本来就不会被
放行的请求，就能耗尽合法调用方的窗口。

### 实例管理员绕过什么

`users.role = "admin"` **绕过权限过滤**（拿到全部 Provider）与**凭证可见性**
（看到全部凭证）。这个角色本来就拥有写 `permissions` 表的操作权，一条拒绝管理员某个
Provider 的规则是他自己就能删掉的规则；把这条绕过写明，可以避免一条写歪的 deny 把
运维锁在自己的实例外面。

它不绕过别的。限流照样生效，预算照样生效，而且**下面那条 OAuth 基线也照样生效**——
这条限制保护的是账号持有者不被他授权的客户端伤害，而管理员的账号恰恰是第三方 token
最不能变成管理凭证的那一个。

### 凭证可见性跟着 key 走，不跟着人走

一个凭证在以下情况下可见：它没有 owner（`Shared`），或者它的 owner 与**调用 key 的
绑定**一致——`api_keys.user_id`、`api_keys.team_id`，或该 key 的有效组织（它自己的
`organization_id`，或它所绑定团队的父组织）。

之所以取 key 的绑定而不是持有者的成员关系：一个用户可以同时属于两个组织，而一个 key
只属于一个。如果由成员关系决定，多组织用户的每一个 key 都能够到每个组织的凭证，也就
永远无法发出一个比持有者更窄的 key。绑定写在行上：客户端既不发送它也无法选择它，而且
预算链与权限主体读的是同一个值，因此"能看到什么""能花什么""谁付钱"不可能互相矛盾。

可见性只收窄引擎的存活凭证集，从不扩大它。结果为空**不是**错误——"什么都不允许"和
"没有东西可允许"是两种不同的失败，后者是解析阶段的 `NoTarget`，属于配置问题而不是
权限问题。

### 预算链

`[api_key?, user, team?, org?]`，缺失的部分跳过，kind 就是裸字符串
`api_key` / `user` / `team` / `org`。core 拿它们逐字匹配
`quotas.owner_kind`，且不认为它们之间有任何层级：链上**任意** owner 的**每一条**启用
预算都会生效，所以这个顺序是日志里的呈现顺序，不是优先级。

`org` 就是 key 行上的 `organization_id` 原样。与凭证可见性不同，绑定到团队的 key 不会
顺带记到该团队的父组织头上：够到共享凭证正是团队的用途，而预算是运维针对某个具名
owner 写下的一行。

OAuth grant 的链与它背后的 key 完全相同——grant 花的是它被签发时所针对的那个账号，
而不是它自己的预算。控制台／门户会话没有 key，因此它的链从 `user` 开始。

### scope，以及 grant 为什么单独一份

`user:{user_id}`，只有 OAuth grant 例外，它是 `grant:{grant_id}`。core 把 scope 用于
凭证亲和，并且从不解析它。

同一个用户的每个 key 共用一个 scope：key 按构造就是同一个人的凭证，key 之间的隔离是
凭证可见性的职责。而 grant 是**另一个程序**在替这个人行事，把它和这个人自己的流量混在
一起，会让用户拿到一个属于他并没有在运行的程序的续写、让一个客户端引用另一个客户端
创建的上游资源，并在 grant 被撤销之后仍把该客户端的绑定留在这个用户身上。

### OAuth 操作基线

access token 是用户交给别人程序的凭证。除非它的 `client_id` 列在
`AppConfig.oauth.cli_client_ids`（默认为空）里，否则它只能执行 `ListModels`、
`GetModel`、`CountTokens`、`GenerateContent`、`StreamGenerateContent` 与
`CompactContent`，其余一律 `403`。把某个 client 写进那份名单，等于运维接受它可以在
整个 API 范围内代表该用户说话。

### 限流失败即关闭

行来自快照，计数来自 cache——多个实例共用同一条限制，按进程计数会把每条限制乘上实例
数。窗口固定且对齐到 epoch：`now - now % (period_seconds × 1000)`，因此每个实例只凭
时钟就能对齐边界，计数器的 key 是 `rl/{row_id}/{window_start}`。

`metric = "concurrency"` 取一个 cache permit 并持有到请求结束；其余 metric 以限额为
上限对计数器 +1，每次恰好 1。permit **不按窗口分键**（key 是 `rl/{row_id}/live`）：它度量
的是此刻在飞的请求，窗口边界并不切分这件事；周期只是 cache 回收"未释放就死掉的请求"所
占 permit 的时限。想限制 **token** 的限制其实是预算而不是限流：token 数在上游应答之前
根本不存在。

**cache 答不上来就拒绝请求**（`429`，且不带重试提示）。`design/cache.md` 明确要求
cache 故障不得降级到本地状态；这里同理——放行会把一次 cache 故障变成"实例上所有限制
全部失效"，那正是攻击者想要、而运维看不见的时刻。

该还的扣减都会还。某一行拒绝时，会把同一次请求中更早的行已经拿走的全部退回。
`Admitted::finish()` 表示请求确实跑过了：窗口计数因此保留，但并发 permit 仍会归还——
permit 度量的是在途请求数，不是已发生请求数。丢弃一个 lease 会归还其上尚未结清的部分；
由于 cache 是异步的而 `Drop` 不能 await，归还是 spawn 出去的，能够 await 的宿主应改
调用 `Admitted::release()`。每份扣减都随窗口过期，所以即便一次归还丢了，也会在边界处
自愈。

## 数据面

`App::call` 与 `App::connect` 就是通往引擎的全部桥接：先准入，再用 `Admitted` 里的每
一项去驱动句柄的 builder——scope、归因、预算 owner、Provider 集、凭证集、会话——并且不
带任何不是在那里决定的东西。引擎永远看不到 `Caller`，所以它无法重新回答这一层已经回答
过的问题；这一层也从不猜测选路，那是引擎的事。

只有一个值两边都要读：模型名。它在这里从 JSON body 里解析出来（句柄自己的那个 helper
是私有的，所以同样的两行在两处各有一份），然后显式传给 builder，这样权限和限流判定所依
据的名字就是选路使用的名字。把模型放在路径里的方言——Gemini 那种形状——已经由宿主解析
过，宿主设置 `DataPlaneRequest::model`。

网关的会话头在转发前会被剥掉，这里剥一次，句柄里再剥一次。客户端不能靠发送网关用来标识
会话的那个头，去挑别人的会话。

### 流还没写完，lease 就不能丢

`CallOutcome` 把 `Admitted` 交还出来是刻意的：

| 扣减 | `finish()` 之后 | drop 时 |
|---|---|---|
| 窗口计数（`requests` 等） | 保留——请求确实发生了 | 不动 |
| 并发 permit | 仍然占用 | 归还 |

`call()` 返回时已经调用过 `Admitted::finish()`，因此**宿主必须在回写响应的全过程中持有
`outcome.admitted`**。并发 permit 度量的是「在途」请求数，而流式响应在 `call()` 返回之
后很久仍然在途；过早丢弃会在本请求还在用这个槽位时，就把它让给下一个请求。能够 await
的宿主应在请求结束时调用 `Admitted::release()`，而不是依赖 drop 路径——`Drop` 不能
await，只能 spawn。

准入之后失败是相反的情况：`call()` 在返回前就把所有扣减退回，因为什么都没跑成。

`SdkError` 映射为 `AppError` 时保留状态码。引擎、store 和 cache 的失败被拆回本 crate
已有的变体——所以 `AppError::Core` 依然能同时匹配到两层各自抛出的预算耗尽——其余的整体
保留在 `AppError::Sdk` 里，因为「所有目标都返回 429」或者「名字解析后没有可用目标」值
多少个状态码，句柄已经判定过了。

## 厂商服务

服务视图（`profile`、用量、插件、远程控制）没有 `OperationKey`，也没有模型。core 自己
就写明它不决定谁是谁的 admin，所以本 crate 只回答两个问题，其余交出去：

1. **目标集的凭证**是该 Provider 下调用方**可见**的那些凭证——和同一把 key 发模型请求时
   能花的是同一组。因此 `Pool` 聚合的只是这个调用方本来就够得着的东西，`Credential(id)`
   也只能点名其中之一（其余是 `404`）；
2. **角色**：实例管理员是 `Admin`；否则必须是**该集合中每一个凭证**所属组织或团队的
   `Admin` 成员。其他一律是 `Member`，而 `Member` 只能要 `Caller` 视图——`Pool` 与
   `Credential` 是 `403`。

「每一个凭证」不是形式：目标就是整个可见集，所以管着一个组织不能渲染出一个还包含另一个
组织凭证的池。无主凭证按定义没有管理员——没有人是「无」的 `Admin` 成员——所以在所有凭证
都共享的单租户实例上，只有实例管理员够得到 `Pool` 和 `Credential`。要放宽是运维的决定
（提升这个用户，或给凭证一个 owner），不是这里替他推断出来的。

服务**不拿限流 lease**——为一次拉取 profile 扣掉一格窗口，花的是调用方真正要用来跑花钱
流量的额度——但会传预算链，因为 `Caller` 视图要据此渲染调用方自己的窗口。报告配额不等于
消耗配额。

`ServiceRequest::user_id` 取自 `attribution(caller).user_id`，和模型请求用的是同一个函
数。core 眼中的调用方要靠它去找这个人的用量；一个请求内部对「这是谁」出现两个答案，正是
单一 `Caller` 规则要防的那个 bug。

宿主用 `DownstreamCapture::open_service` 记录渠道服务的下游 HTTP 交换和 WebSocket 帧，
归属来自已认证的调用者，不获取模型准入租约，也不结算用量。Core 服务路径仍在模型上游尝试
采集链路之外，因此服务记录没有上游关联，不代表它一定没有向厂商发起请求。

## 下游 capture

`capture_records` 有两侧，归属也是两个。core 的 `StoreObserver` 为每一次真实发送写
`side = Upstream` 的行，以及结算后的 `usage_records` 行；入站的 HTTP 交换只有宿主看得
见，所以 `side = Downstream` 那一行是本 crate 的。`design/core-observation.md` 把规则写
死了——宿主拥有下游记录后再建立边，Core 不伪造下游记录——`src/capture.rs` 就是这一半。

| | 上游（core） | 下游（这里） |
|---|---|---|
| 开关 | `enable_upstream_log` | `enable_downstream_log` |
| 正文开关 | `enable_upstream_log_body` | `enable_downstream_log_body` |
| 遮蔽 | `disable_log_redaction` | 同一个开关，同一张字段名单 |
| id | 自己的，不透明 | **请求 id**，也就是 usage 行的键 |
| 正文存放 | `capture_events`，流式 | 内联列，缓冲 |
| WebSocket 帧 | `capture_events`，一帧一行 | `capture_events`，一帧一行，缓冲 |

新实例上这四个开关都是关的。记一次交换，每个请求就要多写两条记录；在只有一个写连接的
SQLite 上，这占了一个请求开销的大头，所以日志要由运维自己打开。

四个开关都从请求所钉住的那个 revision 的 `settings` 行读出，与装配 `AppData` 是同一次
读——不从 `AppConfig` 读，也不另起一套。

`App::call` 在准入之后打开 capture（归属列就是准入的决定，因此在那之前被拒的请求没有
记录），并把它连同已经记好的响应头一起放在 `CallOutcome` 里交还。宿主把写出去的分片喂
给它，再结束它：

```rust
capture.record_response_chunk(&chunk);
capture.settle(store, CaptureOutcome::Complete, usage).await;
```

`settle` 会 await `UsageCompletion`，而那正是让 core 结算的东西；想自己拿 `UsageReport`
的宿主就自己 await，然后调 `link_exchanges` 再 `finish`。

### 一条 socket 是一行记录加一串帧

升级了请求的宿主用 `101` 调 `record_response_head`，把这一行变成 `ws_connection`，
随后每转发一条消息调一次 `record_frame`：

```rust
capture.record_frame(CaptureDirection::Request, CapturedFrame::Text(text));
```

形状是 schema 定的，不是本 crate 定的：「所有 WS 消息都追加到 WsConnection 上」，
而 `sequence` 是「宿主分配的单调序，跨整条 WS 连接的两个方向，包含控制消息与并发
轮次」。所以一条 socket 是**一行**记录加一串跨两个方向的有序事件，而不是一次交换
一行。

`turn_id` 留空。这一列标识的是「可识别时的 WS 业务轮次」，而「轮次」是某个方言的
概念——OpenAI 的 `response.created`/`response.done`、Gemini Live 自己的一套——转发
不透明帧的宿主看不见它。替线上协议发明一条它没画过的边界，只会在日志里留下一行
谁都没产生过的 `WsTurn`。

帧和别的正文一样受 `enable_downstream_log_body` 控制——帧本来就是 socket 的正文——
开启后保留收到的全部帧，在 socket 结束后写入日志。

### 先问再拷

`enable_downstream_log` 关闭时，`DownstreamCapture::open` 返回 `None`：不分配、不拷贝
正文、不写行。`enable_downstream_log_body` 关闭时正文同样不拷贝，`*_body_state` 记作
`NotCaptured`——读的人据此分辨「本来就没有正文」和「我们选择不留」。开启后保留收到的完整
正文，日志详情也完整返回已存正文；`Partial` 表示交换中断，而非日志大小截断。

遮蔽按 core 自己那张名单处理头名、查询参数和 JSON 字段（`authorization`、`cookie`、
`api_key`、`access_token`……），使一个请求的两侧藏起同样的东西；core 的辅助函数是私有的，
所以这里是同一张名单的第二份实现，而不是调用。请求正文写入前脱敏，不按日志大小截断。
不是 JSON 的正文没有键可匹配，按收到的样子存——这也是正文开关默认关闭的理由之一。

### 边

`capture_links` 记的是「这个请求碰过哪些上游」，重试过的请求每次尝试一行。两个来源合并，
因为单独任何一个都不完整：

- `UsageReport::exchanges`，即 core **计得出量**的那些。被改投别处的 `503` 没有用量，
  所以根本不在报告里；
- `initiator_request_id` 等于本请求的那些 `capture_records`——schema 保留这一列正是为了
  让重试可追踪。它会漏掉*续传*碰到的上游记录：那条记录带的是另一个请求的发起者，只有报告
  知道本请求也碰过它。

`sequence` 是 `(started_at_ms, attempt_ordinal)` 上的稠密排名，不是上游自己的序号：
换 Provider 重试会让引擎的尝试计数从头开始，于是同一个请求的两次尝试都可能是序号 1。
同一毫秒内开始的两次尝试共用一个号，这正是该列所说的「并行调用可以共享序号，展示时用
upstream_id 打破并列」。

交换结束前什么都不写：记录和它全部的边在同一个 batch 里落库。`enable_upstream_log` 关闭
时一条边都不写——没有上游行可指，外键会把记录一起带走。

### capture 失败永远不是请求失败

与 core 同一条规则。`finish` 以 `error` 级记录后返回；到那时请求早已被回答，`Err` 不能
变成响应。`App::call` 在自己的失败路径上丢弃它。若 batch 因为某条边指向了从未写入的记录
而失败，则单独重试写记录本身：为一条缺失的边丢掉请求自己的日志行，是两者中更糟的那个。

下游行的 `provider_id`、`credential_id` 和 `metrics` 保持为空。重试过的请求
碰了两个 Provider、两份凭证，单个列只能二选一；边不必二选一就能回答，而计费用量是
`usage_records` 那一行。

## 发布链接

core 拥有已发布的字节和它们的 id，但它没有对外的 HTTP 面，所以说不出这些字节能从哪里取
回。`AppPublicationUrl` 给出 `{public_base_url}/publications/{id}`，
`App::read_publication` / `App::delete_publication` 服务那条路由。

**没有配 `public_base_url` 时它返回 `None`**，于是发布会在写入任何 body 之前以
`Unsupported` 失败，调用方被告知改要内联字节（`b64_json`）。另一种做法是用请求的 `Host`
——那是客户端选的——或者用前置代理给的 `x-forwarded-proto` 去拼链接。用猜出来的东西拼出
的链接比没有链接更糟：上游回答被接受、字节被存下、URL 交了出去，然后它在别的地方 404。

读不到、body 已释放、已过期，三者是同一个回答——`404`——因为区分它们等于告诉探测者哪些
id 存在过。这里不校验 scope：id 本身就是随机的秘密，链接即凭据，和 core 自己的读取一致。
## 身份操作

`Operations::new(&gproxy, &data, &config)` 按家族分发访问器。每个家族都是同一套泛型
形状给出的 `list / get / create / update / delete`，外加该家族确实需要的额外操作。

| 家族 | 表 | 五件套之外 |
|---|---|---|
| `users()` | `users` | `set_password`、`clear_password`、`set_allowlist`、`batch` |
| `api_keys()` | `api_keys` | `create` 只返回一次明文，另有 `reveal`、`rotate` |
| `organizations()` | `organizations` | `batch`；删除时执行 key 级联 |
| `teams()` | `teams` | `batch`；删除时执行 key 级联 |
| `members()` | `organization_members` | 复合主键上的 `add`、`set_role`、`remove` |
| `team_members()` | `team_members` | 复合主键上的 `add`、`set_role`、`remove` |
| `permissions()` | `permissions` | `batch`——规则集整体编辑 |
| `rate_limits()` | `rate_limits` | `batch` |
| `oauth_clients()` | `oauth_clients` | 用 `retire` 代替 delete |
| `sessions()` | `user_sessions` | `list`、`revoke`、`revoke_all`、`purge_expired` |
| `audit()` | `audit_events` | `record`、`try_record`、`query` |

`Operations` **不做任何授权**。谁可以调用哪个家族，是宿主中间件在操作运行前依据
`Caller` 做出的决定——自 scope 模型起，还依据 guard 解析出的 `AdminScope`。见下节。

## 管理 scope

`organization_members.role` 与 `team_members.role` 从 schema 写下那天起就存着
`MembershipRole::Admin`，却没有任何东西消费它：管理面要求实例管理员，门户严格自助，
于是组织管理员根本没有面。现在他们有了**同一个**面，只是被一个带类型的值收窄。

```rust
pub enum AdminScope { Instance, Organization(String), Team(String) }
```

每个请求一个，派生只在一处——`AdminScope::resolve`，宿主的 guard 是它唯一的调用方：

| 调用者 | scope |
|---|---|
| `users.role = admin` | `Instance`；不读 scope 头 |
| API key（含 OAuth 授权的内部 key） | key 自己的 `team_id`，否则 `organization_id` |
| 控制台会话 | `x-gproxy-admin-scope` 头（`SCOPE_HEADER`），按调用者的 **admin** 成员关系校验 |

API key 这条就是数据面已经在用的那条规则：key 的绑定一次决定计费归属、权限主体和
凭证可见性，所以它也决定这个；key 请求上的那个头被忽略而不是被拒。只管理一个 scope
的会话不需要这个头；管理多个的在选定之前拿到一个点名这个头的 `400`；一个都不管理的
整面 `403`。

### 它收窄查询，而不是核对答案

这就是全部要点，也正是门户安全性的镜像。门户没法点名别人，因为根本没有那个参数；
这个面没法够到 scope 之外，因为根本构造不出那样一条查询：

| 什么 | 怎么做 | scope 之外 |
|---|---|---|
| 列表 | `AdminScope::narrow` 在语句构造前重写 `(ownerKind, ownerId)` 筛选 | **空页**——一个没有行的合法问题 |
| 按 id 取行 | `AdminScope::admits` 判定该行的 owner | **`NotFound`**，绝不是 `Forbidden`——后者会确认这个 id 存在 |
| 写入点名 owner | `AdminScope::admit_write` | **`Forbidden`**——这个 id 是调用者自己填的，不是发现的 |

patch 校验两次，按当前的行和按改完的行，所以一行既不会被推出 scope，也不会被拉进来。

包含关系：实例 scope 装下一切，包括无主的行；组织装下自己的行**以及它的团队的行**，
但不包括成员个人的行；团队只装自己的。`AdminScope::organization` 会报出团队的父组织，
那正是控制台面包屑要渲染的东西。

### section 表

哪些家族对哪些 scope 开放，只在 `ADMIN_SECTIONS` 里声明一次。宿主的路由表给每条路由
点名一个 section 并调 `require_section` 把关，而它对不认识的 section 一律拒绝——所以这
张表**默认是关的**，点名了未声明 section 的路由只有实例 scope 到得了。
`GET /admin/api/context` 按调用者的 scope 渲染同一张表，控制台的导航只从它来。

现在开放的：`context`、`session`、`credentials`、`quotas`。仅实例：所有配置网关本身的
家族，以及——暂时——每一个身份家族。开放某一个是一次有意的动作而不是一个开关：它们中
多数要的是对成员关系做 `IN`，而不是一次列比较。

`ScopedManage` 就是这两个开放的配置家族 `credentials` 与 `quotas`，包了一层，让上面
那些规则在 `Gproxy::manage()` 之前跑。这是本 crate 唯一一处包裹 sdk 家族的地方，它配
得上这层包裹：它有东西要判断——这行归谁——而引擎绝不该知道这件事。`quotas` 一张表同时
装着租户预算（`owner_kind` 为 `org`/`team`）和运维限额（`credential`/`provider`）；
分界在 owner kind 上，不在路由上。

### 一个 batch、一个 revision，然后一次通知

一次配置写入就是一次 `Store::commit_revision`：调用方的语句与
`settings.config_revision` 的自增要么一起落库、要么都不落。只落了行而没有自增，对等实例
看不见；只自增而没有落行，它们白重载一次。create 与 update 都在**同一个事务内**把行读
回来，所以返回的 DTO 是该 revision 留下的那一行，而不是一次可能已被对等实例改过的重读。

写入之后，writer 在 `gproxy-core` 的失效主题上发布
`Invalidation::ConfigurationChanged { revision, scopes }`，scope 用本 crate 自己的名字：
`identity`、`permissions`、`keys`、`rate_limits`、`oauth_clients`。
发布是尽力而为——cache 拒收只让部署多等一个轮询间隔，不影响正确性。

**这里与 sdk 的 `manage::Writer` 有何不同，为什么。** sdk 先重载再通知，因为对等实例不能
在收到通知后发现写入方自己还在服务它刚宣告的那个 revision。本层根本不重载：身份行不在
`CoreData` 里，它们喂养的是 `AppData`，而 `AppData` 归宿主所有——宿主持有 `AppSnapshot`，
并通过 `App::refresh()` 重建它。writer 自作主张重载会发布出第二份相互竞争的快照，
`AppSnapshot` 要保证的单调性就此丢失。所以契约是：**操作负责提交与通知，宿主负责刷新。**

有两个家族不是配置，因此刻意两件事都不做。`user_sessions` 不在 `IdentityData` 里，认证在
每个请求上都读表，撤销无需任何东西重载即可生效；`audit_events` 是历史，没有任何请求路径
会读它。为这两者自增 revision，等于让每一次登录和每一条审计都让整个集群为一个快照里根本
不存在的变化失效一次。

### 什么绝不会被返回

`users.password_hash`、`api_keys.key_hash`、`api_keys.secret` 的字节和
`user_sessions.token_hash` 没有任何 DTO 字段，也没有任何访问器。user 只报
`hasPassword`，key 只报用于展示的 `prefix` 与 `hasSecret`，session 只报开启时刻与结束时刻。

key 的明文在响应里只出现两次：`create` 与 `rotate` 各返回一次。`reveal` 只有对创建时带了
`retainSecret` 的 key 才能事后给出明文——那份副本由 core 的 `SecretCodec` 绑定该 key 自身的
id 密封，因此把密封块拷到别的行上是打不开的。保留默认关闭：实例复现不出来的 key，数据库
泄露时也交不出去。

### 显式的 key 级联

删除组织时，在同一个 batch 里、且在父行之前删除：绑定到其任一团队的 key、绑定到该组织本身
的 key，以及这些团队。删除团队时删除绑定到它的 key。这正是下一节存在的理由，也是它被写死
在代码里而不是交给 schema 的原因。

选择删除而不是解绑是刻意的。解绑会把一个 key 从"这个团队的凭证"悄悄放宽到"它的用户能碰到
的一切"——一次删除动作造成的权限**扩大**；而实体自身的契约写明：owner 链与可见性边界都已
消失的 key 必须停止服务请求。由于显式语句先执行，**确实**带外键的数据库会发现无物可级联，
于是新建库与升级库落到完全相同的状态。

### schema 表达不了的校验

- **key 的绑定检查三件事**：组织与团队存在、key 的用户是它们的成员、两者同时设置时团队的
  父组织*就是*该组织。patch 会按行最终会持有的绑定重新校验，而不是只看 patch 提到的那一半。
  在升级路径上这两列根本没有外键，所以三者里有两者只有这一道检查。
- **权限规则与限流规则都只接受一个主体。** 一个都没有，是 `PermissionSet::build` 会直接丢弃
  的行；两个都有，是快照仍然承认的交集——v3 库里可能存在这种行——但没有人写规则时是这个意思，
  因此写入时就拒绝，而不是存下来日后被误读。
- **`action` 是 `allow` 或 `deny`**，`role` 是 `admin` 或 `user`，成员角色是 `member` 或
  `admin`。
- **`periodSeconds` 为正**，**`limitValue` 不为负**；0 是合法上限，表示不删行地关停某个主体。
- **最后一个启用的管理员不能被删除、禁用或降级。** 实例角色不是成员身份，不是管理员的人无法
  授予它，所以一旦丢失，只能去数据库里救。计数从数据库读而不是从快照读：落后一个 revision 的
  快照仍会列出刚被删掉的管理员，于是放行对真正最后一个管理员的删除。
- **改密码会在同一个事务里结束该用户的全部会话**——会话活得比开启它的凭证更久，正是一次重置
  要关掉的那个窗口。
- **OAuth 客户端只能 retire，不能删除**，retire 通过 store 的原子 `retire_many` 撤销它的授权、
  授权的内部 key 和已签发的全部令牌。同一个 `client_id` 重新注册会被拒绝，因此新行永远不可能
  继承上一次注册攒下的授权。

### 审计，以及 `detail` 绝不会携带什么

`audit().record(AuditEntry { … })` 在 revision batch **之外**追加一行。这不是偷懒：否则一次
失败的流水插入会把它只是在描述的那次操作一起回滚，而**被拒绝**的操作——恰恰是事后调查最想要的
那一行——根本没有事务可加入。写流水失败只记日志，绝不向上抛。

`AuditEntry::redacted(value)` 是把调用方提供的 JSON 放进条目的唯一受支持方式，因为脱敏发生在
存入之前而不是展示之前：进了列的密文早已泄漏进每一份备份和副本。它会递归走遍对象与数组的每一层，
把名字（忽略大小写、下划线与短横线后）属于下列之一的字段的值替换掉：

```text
accessToken  apiKey     authorization  clientSecret  code        codeVerifier
cookie       credentials  currentPassword  idToken    key         keyHash
masterKey    newPassword  oldPassword   password     passwordHash  privateKey
refreshToken secret      sessionToken   setCookie    token       tokenHash
verifier     xApiKey
```

替换成字面量 `[redacted]`——是替换而不是删除，这样读者能分辨"这次操作带了密码"和"这次没带"。
匹配的是归一化后的完整名字而不是子串，所以 `passwordPolicy` 或 `keyboardLayout` 会原样保留。
`AuditEntry::failed(&error)` 记录错误的稳定 **code** 而不是它的 message，因为 message 可能把
调用方发来的值原样引回去。

`query(AuditQuery)` 按最新在前分页——`created_at_ms DESC` 再按主键，这样同一毫秒内写下的两行
不会在翻页边界上重复或漏掉。

## OAuth issuer

`Operations::issuer()` 是 gproxy **面向下游客户端**充当 OAuth 授权服务器——编辑器插件、
编码 CLI、用户机器上的脚本。它**不是** gproxy 登录上游的那条路；那是 sdk 的 `login()`，
在那里 gproxy 是别人 OAuth 的客户端。本模块里没有任何东西会和 Provider 通信。

```text
  Claude Code ──authorize/token──▶ gproxy  (operations::issuer：服务器)
                                     │
                                     └──login()──▶ OpenAI   (gproxy-sdk：客户端)
```

| 操作 | 宿主映射到的端点 | RFC |
|---|---|---|
| `authorize_details(caller, query)` | `GET /oauth/authorize`——先校验，再画同意页 | 6749 §4.1.1 |
| `authorize(caller, query, decision)` | `POST /oauth/authorize`——一个 code，或一次错误重定向 | 6749 §4.1.2 |
| `token(request)` | `POST /oauth/token`——`authorization_code`、`refresh_token`、`urn:ietf:params:oauth:grant-type:device_code` | 6749 §4.1.3/§6、8628 §3.4 |
| `device_code(request, origin)` | `POST /oauth/device/code` | 8628 §3.1 |
| `device_details(user_code)` | 批准页 | 8628 §3.3 |
| `device_decision(caller, user_code, decision)` | 批准页上的按钮 | 8628 §3.3 |
| `device_cancel(client_id, device_code)` | 设备主动作废自己打印出来的码 | — |
| `revoke(request)` | `POST /oauth/revoke` | 7009 §2 |
| `metadata(origin)` | `GET /.well-known/oauth-authorization-server` | 8414 §2 |

本模块不是 HTTP：这里没有路由、没有表单解析、也没有 `Location` 头。
`error_body(&AppError)` 是让两个宿主把失败渲染成同一个样子的那一个函数——
`AppError::OAuth` 保留它携带的 RFC code，凡是实例自己的锅一律变成 `server_error`，
其余任何情况都不会把内部 message 引给客户端。

### 一次授权（grant）是什么

一次授权绑定一个**用户**和一把 `kind = OAuth` 的**内部 API key**，两者在同一个事务里创建。
再往下的一切都只认 key：预算链、凭证可见性边界、权限主体，全部来自那一行 key，和用户自己手工
铸的 key 走完全相同的路。这把 key 永远无法被出示——认证层拒绝把 `kind = OAuth` 的 key 当作
bearer key——所以唯一的用法是出示 access token，而它在一条语句里就把整条链解析完。

**OAuth 调用方随后能做什么不在这里决定。** 那条操作基线——列模型、取模型、计 token、生成、
流式生成、压缩，除非 client 写进了 `oauth.cliClientIds`，否则别无其他——住在
[准入](#oauth-操作基线)里，因为它是这个令牌**每一次请求**的属性，而不是它被铸出来那一刻的属性。
放宽它不应该要求重新签发令牌，收紧它必须立刻作用于已经存在的令牌。

### 这个 issuer 不让步的几条规矩

**PKCE 永远是 S256。** `plain` 被拒绝，缺失 challenge 同样被拒绝。注册在册的 client 全是
公共客户端，没有 secret 可以证明自己，所以 verifier 是泄漏的 code 与令牌之间唯一的东西。
`code_challenge_method` 的**拼写**读得宽松（去空白、忽略大小写），因为要指认的变换只有一种。

**redirect URI 精确匹配。** 不是前缀，不是通配，也不是"同源"。每一种更松的规则都会以同样的
方式失败：注册了 `https://app.example/cb` 的 client 会连
`https://app.example/cb.evil.example`、`https://app.example/cb/../../elsewhere`、
`https://app.example/cb?next=//evil` 一起接受，而这些都是攻击者能控制、又能收到授权码的 URL。
注册表拒绝存 `*`，出于同一个理由。

**code、refresh token 与 device code 都是一次性的。** 消费与换发是 store 的
`exchange_tokens_many` 里的一个原子 batch，所以同一份凭证的两次兑换无论怎么交错都不可能同时成功。

**令牌只在一次响应里存在。** 来自 `getrandom` 的 32 字节，base64url 无填充；行里只留 SHA-256——
`oauth_tokens.token_hash`、`oauth_codes.code_hash`、`oauth_devices.device_code_hash` 都是
`Binary(32)`。哈希函数和 `api_keys.key_hash`、`user_sessions.token_hash` 用的是同一个，
只有编码不同，因为那两列是文本列。丢失的令牌只能换发，永远找不回来。

### 轮换重放会撤销一整族

code 和 refresh token 都是一次性的。当其中之一被出示第二次时，只有两种可能，而站在这里没有办法
分辨：客户端丢了响应在重试，或者别人也拿到了这份凭证。RFC 6749 §4.1.2、RFC 6819 §5.2.2.3 与
OAuth 2.0 Security BCP §4.14 的结论一致——按泄漏处理。

所以被重放的 refresh token（或授权码）回 `invalid_grant`，**并且**在一个 batch 里撤销这次授权、
它的内部 API key，以及它曾签发的每一个 access 与 refresh token。合法客户端此刻手上的 access token
同样立刻失效，它会重跑一次登录；小偷的也失效，而它跑不了登录。

检测靠的是 store，而不是"先读再判"：兑换语句带着 `consumed_at_ms IS NULL` 去消费那一行，
所以花掉的凭证会以 `CasOutcome::Conflict` 回来，再回读一次那行就能区分它是被消费过（重放），
还是仅仅过期或被撤销（普通的 `invalid_grant`）。

**设备流是唯一的例外。** 轮询的客户端本来就会反复发同一个 device code，而丢失的响应和送达的
响应从这里看不出区别。被消费过的设备授权只回 `invalid_grant`，到此为止——device code 反正已经
花掉了，再把令牌一起带走，只会把一个刚刚合法连上的客户端锁在账号外面。

### 设备流复用了授权码流

一次批准铸出的是一个普通授权码，而 store 的 `issue_many` 在**同一个** batch 里预留并批准那行
待决的设备授权。两个列让这件事不需要第二套形状：

- 授权码的 `redirect_uri` 是 `urn:ietf:params:oauth:grant-type:device_code`，一个任何注册都
  不可能持有的 URN（注册要求含 `://`），因此设备铸出的码永远不可能走 `authorization_code` 兑换；
- 授权码的 `code_challenge` 是 `device_code_hash` 的 base64url，而它*就是* device code 的 S256
  challenge，因为两者是同一个 SHA-256 的 base64url。于是 device code 本身就是 PKCE 的 verifier，
  不花任何额外代价。

user code 是八个符号，字母表里去掉了 `I`、`O`、`0`、`1`，展示成 `ABCD-EFGH`，查询时归一化，
所以字符对的任何一种写法都能命中。`slow_down` 永不发出：要回答它就得在每台设备的每次轮询上写一次库，
而给端点限流是宿主的事，也是同一道防线。

### `issuer` 来自挂载点，而不是某个请求头

同一个实例最多在三个挂载点上提供 issuer——`https://host`、`https://host/{namespace}/v1`、
`https://host/{provider}/v1`——而 RFC 8414 §2 要求 `issuer` 标识必须恰好是客户端取到这份文档
的那一个。只有宿主知道请求落在哪个挂载点上，所以 `IssuerOrigin` 是**传进来的**，绝不在这里算。

scheme 是这件事的另一半。`x-forwarded-proto` 是任何客户端都能发的头，所以宿主**只能在对端位于
`trustedProxies` 时**相信它，否则退回 `publicBaseUrl` 或套接字自身的 scheme。用攻击者可控的
`Host` 或 `x-forwarded-proto` 拼出来的 issuer 标识，就是一份指向别人端点的发现文档。

`IssuerOrigin` 把 origin 和 mount 分开存，因为两者用法不同：协议端点挂在 mount 下，客户端才能
发现自己正在对话的那一个；而设备验证页挂在 origin 上——无论数据面挂了多少个点，门户只有一个。

### 这些全都不动 revision

签发、刷新、撤销都**不**推进 `settings.config_revision`，这和 `user_sessions`、`audit_events`
是同一个决定。授权在每次请求时都由一次数据库读解析，所以对等实例过期的 `AppData` 既不会放行
已撤销的令牌，也不会拒绝还活着的令牌。反过来，每次兑换都推 revision，等于**每一次刷新都让全集群
的快照作废**——在 access token 一小时一换的机群上，这是一场毫无收益的重载风暴。唯一算配置的
OAuth 操作是 retire 一个 client，因为它改的是 `AppData` 里的 allowlist，它在 `oauth_clients()`
里提交 revision。

每一次签发、拒绝、刷新、重放与撤销都用 `try_record` 写进审计流水，而这是有承重作用的而非偷懒：
一次写流水的失败如果把成功的兑换变成 500，客户端就会拿着已经花掉的 code 重试，重放规则随即会
撤销它刚刚拿到的授权。

## 门户

上面那些家族是运营者的工具：给一个 id，它就作用在那个 id 指向的行上。门户是产品的另一半
——持有 key 的那个人登进来，看自己的 key、自己的花费、自己能调的模型，以及自己授权过的程序。

```rust,ignore
let portal = Operations::new(&gproxy, &data, &config).portal(&caller);
portal.context().await?;              // 门户最先加载的那一个调用
portal.models()?;                     // 全部名字，每个都标了 `permitted`
portal.keys().create(write).await?;   // 只为调用方铸造，不为别人
portal.usage(query).await?;           // 调用方自己的聚合
portal.quota().await?;                // 调用方自己的预算链
portal.recent_requests(20).await?;    // 精简过，且受开关控制
portal.oauth_sessions().list().await?;
portal.password().change(change).await?;
```

| 操作 | 返回 |
|---|---|
| `context()` | 用户、组织、团队、功能开关 |
| `models()` | 全部暴露名与 `渠道/模型` 形式，各带 `permitted` |
| `keys()` | 对调用方自己的 key 做 `list`、`create`、`rotate`、`reveal`、`delete` |
| `usage(query)` | 汇总，可选的分组切片，可选的趋势 |
| `quota()` | 调用方预算链上每条预算的当前窗口 |
| `recent_requests(limit)` | 调用方自己的近期请求，精简版 |
| `oauth_sessions()` | 对调用方自己的授权做 `list`、`revoke` |
| `password().change(..)` | 先验旧密码，再设新密码，然后到处登出 |
| `sessions()` | 调用方在哪些地方登录着 |
| `logout(token)` | 结束本次请求所在的那个会话 |

`Operations::portal_login(name, password)` 与 `portal_logout(token)` 挂在 `Operations`
上而不是 `Portal` 上，理由只有一条：登录发生在**还没有调用方之前**，而一个全部契约就是
"每个方法都锁定在这个调用方身上"的类型，不可能同时容纳那个先于调用方发生的方法。
**登录尝试的限流是宿主的事。** 本 crate 从不解析转发头，因而没有客户端地址；用别的东西
做键，要么把一个用户名全局锁死，要么根本限不住任何东西。

### scope 由构造保证，而不是靠检查

**门户的任何方法都不接受 user id。** 没有任何一个参数能让调用方指名别人——这就是整个设计：
检查可能被忘掉，不存在的参数忘不掉。

唯一看起来像例外的请求字段是 `PortalUsageQuery.userId`，它存在只是为了让控制台能把同一个
过滤对象投给两个面。在查询到达引擎之前，它会被**覆写**成调用方自己的 id——是写进过滤器，
不是拿来比对。测试就往那里塞另一个用户的 id，然后断言它被忽略了。

确实绕不开 id 的地方——key、grant——先读行、把 owner 与调用方比对，然后才动手。

### `NotFound`，绝不是 `Forbidden`

门户操作作用在别人的行上时回答 **404**。403 会确认这个 id 确实存在，而这恰恰是枚举攻击
要探的那件事；从外面看，别人的 key 和一个从未铸造过的 id 必须是同一个答案。同一个 404 也
覆盖 grant 的内部 `oauth` key——门户根本不管理它。

`Forbidden` 在这个面上确实会出现，只用于那件不指名任何行的事：**OAuth grant 调用方**不得
铸造、轮换、揭示或删除 key，也不得修改账号密码。用户交给别人程序的一个令牌，不能凭这份
信任再铸出一把新的长期凭证，也不能把账号主人锁在门外。这是关于**手里这把凭证是什么种类**
的策略，调用方被告知后可以行动，而 `context().features` 会提前报出来，让门户直接隐藏按钮，
而不是渲染一个必定被拒的按钮。

### 它复用管理面家族

门户建 key 就是 `api_keys().create`，只是把调用方自己的 user id 填了进去，于是三条绑定
规则——组织与团队存在、调用方是它们的成员、两者同时设置时团队的父组织就是该组织——仍然只
在一个地方校验。门户用户写了一个自己不属于的组织，是被那道检查拒掉的，不是被这里的第二份
拷贝拒掉的。改密码是在旧密码被证明之后调 `users().set_password`，这也是它会结束**全部**
会话（包括提出请求的那一个）的原因：会话调用方按设计不携带会话 id，而到处登出本来就是一次
改密码应该做的事。

### 门户刻意比管理面看得少

`recent_requests` 不返回体、不返回头、不返回 URL、不返回客户端地址、不返回凭证 id、也不
返回 Provider id——只有 `requestId`、调用方自己的 `apiKeyId`、模型、操作、Provider 的
**展示名**、状态码、capture 状态和时间。

被捕获的体里可能是调用方自己的 prompt，那是他的；但同样可能是系统提示词、工具定义，或者
属于运营者的上游错误，而读取侧的任何过滤都分不出这两者。凭证 id 与 Provider id 是账号持有人
用不上、攻击者用得上的基础设施。Provider 以名字的形式保留，因为"这次是哪个上游服务的"是个
合理的问题。运营者的日志视图全都保留——它就是干这个的。

这份列表由 **`settings.portal_recent_requests_enabled`** 控制，本 crate 里只有这一处读它。
关掉时返回空列表而不是错误：这个开关是运营者对"门户展示什么"的决定，不是对这个调用方的
评价，回 403 只会让他去找一个其实并不缺的权限。`context().features.canSeeLogs` 携带同一个
值，于是页签可以直接隐藏。

### 模型列表不省略任何名字

`models()` 列出每一个暴露名和每一个 `渠道/模型` 形式，并为每个标上 `permitted`。调用方
规则够不到的名字仍然留在列表里，只是 `permitted: false`。v3 会把这种行丢掉，这里不丢：
静默省略会让"这个模型 404"和"你没被允许用这个模型"变成同一个观察结果——而且也没什么要保护
的：暴露名和 `渠道/模型` 形式是实例配置，就是运营者发布出去的那些字符串。被扣下的是它们
背后的 Provider id；DTO 只报一个数量和一个渠道，这说明了一个名字有多冗余，却没点名任何机器。

`Provider名/模型` 同样列出，它与使用 Provider 专属 API 地址并填写原模型名等价。
Provider 改名后，模型前缀和专属 API 地址也随之改变。

`permitted` 由 `admission::permission::allowed_providers` 算出——正是请求漏斗调用的那个
函数，对同一份快照——按 `GenerateContent` 评估，因为门户问的是"我能把 prompt 发给什么"。

## 只在新建数据库上存在的级联

`api_keys.organization_id` 与 `api_keys.team_id` 声明了 `on_delete = "Cascade"`，
但 SQLite 的增量升级路径用 `ALTER TABLE ADD COLUMN` 加这两列，而这种语句无法带外键。
只有全新创建的数据库才有该约束，升级上来的没有，其他后端未验证。

**因此删除组织或团队时，由本 crate 自己在同一个 batch 里、在父行之前删除受影响的
API key**，见[显式的 key 级联](#显式的-key-级联)。一个预算链、权限主体和凭证可见性边界
都已不存在的 key 不能继续服务请求，而在升级上来的实例上，这一层以下没有任何东西会拦住它。

## 类型导出

`ts` feature 给 `dto` 导出的每个类型生成一份 `ts-rs` 声明，由一个测试写出去：

```sh
GPROXY_TS_OUT=console/src/generated/app \
  cargo test -p gproxy-app --features ts export_types
```

不设 `GPROXY_TS_OUT` 时这个测试立刻返回、什么都不写，于是
`cargo test --all-features` 保持无副作用，生成目录只会在有人明确要求时被重写。设了之后，
目录先被清空——一个已经不存在的 DTO 留下的陈旧声明，会在 Rust 侧删掉很久之后仍然让控制台
通过类型检查——最后写一份 re-export 全部的 `index.ts`。另一个测试读 `dto/mod.rs`，当导出
清单与 `pub use` 条目对不上时失败：加了 DTO 却忘了清单，是一个红测试，而不是控制台悄悄少
一个类型。

这个 feature 同时打开 `gproxy-sdk/ts`，因为门户的形状是由 sdk 的形状搭起来的——
`PortalUsageDto` 里装着 `UsageSummaryDto`——而字段类型没有声明的类型，自己也写不出声明。

### 两个 crate，两个目录

导出会在写之前清空输出目录，所以两个 crate 导向同一个目录会互相抹掉。sdk 的声明写到
`console/src/generated/sdk`，本 crate 的写到 `console/src/generated/app`，各有自己的
`index.ts`；`console/` 下的 `pnpm types` 依次跑这两条命令。`ts-rs` 导出一个类型时会连同它
的依赖一起导出，所以两个目录各自自洽，谁也不跨目录 import——代价是门户 DTO 用到的那几个
sdk 形状在两边各有一份声明。
