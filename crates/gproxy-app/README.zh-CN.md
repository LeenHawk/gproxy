# gproxy-app

[English](README.md) | 简体中文

GPROXY v4 的产品层：谁在调用、允许他做什么，以及改变这两者的操作。它拥有身份
（用户、网关 API key、组织、团队、权限、订阅、限流、OAuth issuer、审计）、准入，
以及带类型的管理面／门户／issuer 操作。

它不拥有任何 server。这里没有 HTTP 框架、没有路由器、没有运行时、没有 CLI，也没有
引擎的任何部分：选路、凭证选择、失败转移、协议转换、结算与捕获都属于 `gproxy-core`
和它之上的句柄。正是这条界线让同一套判断既能跑在原生 axum server 后面，也能跑在
Cloudflare Worker 里——本 crate 能为 `wasm32-unknown-unknown` 构建，而且会一直如此，
因为它本来就没有任何与平台绑定的东西。

## 一个请求的形状

```text
传输层（宿主）
  → auth        Caller   { 用户、角色、api key、组织、团队、订阅、grant }
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
| `publication` | `AppPublicationUrl`，以及宿主下载路由背后的读取与删除 |
| `operations` | 身份写入家族，每次写入一个 revision commit 加一次对等实例通知 |
| `dto` | 这些家族交换的线上形状：String id、毫秒时间戳、camelCase，不含任何密文 |
| `audit` | 只追加的审计流水，写在 revision batch 之外，附带脱敏规则 |
| `error` | `AppError`，带 `status_code()` 与用于 API 信封的稳定 `code()` |

只剩 `capture` 还是只有声明和写明的契约，后续阶段就地填充，不需要再改 crate 的形状。

## 快照

`AppData` 之于本 crate，等同于 `CoreData` 之于 `gproxy-core`，而且两者刻意来自同一次
读取：`Store::load_all_data()` 在一个 batch 里返回 control、routing、identity 三组行，
因此引擎与产品层不会在一个请求中途差出一个 revision。一个请求只取一次
`AppSnapshot::load()`，并在整个生命周期里持有那个 `Arc`。

| 索引 | 回答 |
|---|---|
| `ApiKeyIndex` | 摘要 → `ApiKeyIdentity`（key、用户、角色、组织、团队、订阅、kind、过期） |
| `MembershipIndex` | 用户 → 带角色的组织与团队，外加每个团队的父组织 |
| `CredentialOwnership` | 凭证 → `Shared` / `User` / `Team` / `Org`，以及调用方能否选中它 |
| `PermissionSet` | `(主体, Provider, 模型, 操作)` → `Allow` / `Deny(原因)`，以及允许的 Provider 集 |
| `ClientAllowlist` | 某用户是否可以授权某个 OAuth client |

此外还以 map 形式带着 `users`、`organizations`、`teams`、`rate_limits`、
`subscriptions`、`plans`、`plan_limits`、`pools`、`pool_members` 与 `oauth_clients`。

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
| `ApiKey` | `Authorization: Bearer`、`x-api-key` 或 `x-goog-api-key` 里的网关 key | key 行上的组织、团队与订阅 |
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
  检查 token、grant、client、用户、内部 key、订阅与 client 允许列表，因此并发的撤销无法
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
只属于一个。如果由成员关系决定，多组织用户的每一个 key 都能够到每个组织的订阅，也就
永远无法发出一个比持有者更窄的 key。绑定写在行上：客户端既不发送它也无法选择它，而且
预算链与权限主体读的是同一个值，因此"能看到什么""能花什么""谁付钱"不可能互相矛盾。

可见性只收窄引擎的存活凭证集，从不扩大它。结果为空**不是**错误——"什么都不允许"和
"没有东西可允许"是两种不同的失败，后者是解析阶段的 `NoTarget`，属于配置问题而不是
权限问题。

### 预算链

`[api_key?, user, subscription?, team?, org?]`，缺失的部分跳过，kind 就是裸字符串
`api_key` / `user` / `subscription` / `team` / `org`。core 拿它们逐字匹配
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
| `subscriptions()` | `subscriptions` | `batch` |
| `pools()` | `subscription_pools` | `batch` |
| `pool_members()` | `subscription_pool_members` | `batch` |
| `plans()` | `subscription_plans` | `batch` |
| `plan_limits()` | `subscription_plan_limits` | `batch` |
| `oauth_clients()` | `oauth_clients` | 用 `retire` 代替 delete |
| `sessions()` | `user_sessions` | `list`、`revoke`、`revoke_all`、`purge_expired` |
| `audit()` | `audit_events` | `record`、`try_record`、`query` |

`Operations` **不做任何授权**。谁可以调用哪个家族，是宿主中间件在操作运行前依据
`Caller` 做出的决定。

### 一个 batch、一个 revision，然后一次通知

一次配置写入就是一次 `Store::commit_revision`：调用方的语句与
`settings.config_revision` 的自增要么一起落库、要么都不落。只落了行而没有自增，对等实例
看不见；只自增而没有落行，它们白重载一次。create 与 update 都在**同一个事务内**把行读
回来，所以返回的 DTO 是该 revision 留下的那一行，而不是一次可能已被对等实例改过的重读。

写入之后，writer 在 `gproxy-core` 的失效主题上发布
`Invalidation::ConfigurationChanged { revision, scopes }`，scope 用本 crate 自己的名字：
`identity`、`permissions`、`keys`、`rate_limits`、`subscriptions`、`oauth_clients`。
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
- **key 只能选自己用户的订阅。**
- **权限规则与限流规则都只接受一个主体。** 一个都没有，是 `PermissionSet::build` 会直接丢弃
  的行；两个都有，是快照仍然承认的交集——v3 库里可能存在这种行——但没有人写规则时是这个意思，
  因此写入时就拒绝，而不是存下来日后被误读。
- **`action` 是 `allow` 或 `deny`**，`role` 是 `admin` 或 `user`，成员角色是 `member` 或
  `admin`，plan 的 period 取 `total / fixed / day / week / month` 之一，`fixed` 窗口必须有
  正的时长。
- **`periodSeconds` 为正**，**`limitValue` 不为负**；0 是合法上限，表示不删行地关停某个主体。
- 两端都已知时，**`startsAtMs <= expiresAtMs`**。
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

## 只在新建数据库上存在的级联

`api_keys.organization_id` 与 `api_keys.team_id` 声明了 `on_delete = "Cascade"`，
但 SQLite 的增量升级路径用 `ALTER TABLE ADD COLUMN` 加这两列，而这种语句无法带外键。
只有全新创建的数据库才有该约束，升级上来的没有，其他后端未验证。

**因此删除组织或团队时，由本 crate 自己在同一个 batch 里、在父行之前删除受影响的
API key**，见[显式的 key 级联](#显式的-key-级联)。一个预算链、权限主体和凭证可见性边界
都已不存在的 key 不能继续服务请求，而在升级上来的实例上，这一层以下没有任何东西会拦住它。
