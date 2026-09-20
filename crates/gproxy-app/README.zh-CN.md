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
  → sdk call()  引擎只按这些执行，下游不再重新推导任何一项
```

每一步都在收窄，下面不会重新打开上面已经收窄的东西：交给引擎的是一个 Provider 集和
一个凭证集，而不是一个待解释的调用方。sdk 桥接本身（`call.rs`、`service.rs`、
`publication.rs`）是下一步；当前本 crate 到 `Admitted` 为止。

## 现在有什么

| 模块 | 内容 |
|---|---|
| `config` | `AppConfig`：纯 serde，两个宿主共用，不碰文件系统也不读环境变量 |
| `snapshot` | `AppData`——某个配置 revision 编译后的身份，以及单调发布它的 `AppSnapshot` |
| `auth` | 请求指认调用方的三条路径，以及三者产出的同一个 `Caller` |
| `admission` | `Caller` → `Admitted`：Provider 集、凭证集、预算 owner 链、scope、会话、限流扣减 |
| `error` | `AppError`，带 `status_code()` 与用于 API 信封的稳定 `code()` |

其余模块（`call`、`service`、`capture`、`publication`、`operations`、`audit`、`dto`）
都已声明并写明各自将承担的契约，后续阶段就地填充，不需要再改 crate 的形状。

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
时钟就能对齐边界，key 是 `rl/{row_id}/{window_start}`。

`metric = "concurrency"` 取一个 cache permit 并持有到请求结束；其余 metric 以限额为
上限对计数器 +1，每次恰好 1。想限制 **token** 的限制其实是预算而不是限流：token 数在
上游应答之前根本不存在。

**cache 答不上来就拒绝请求**（`429`，且不带重试提示）。`design/cache.md` 明确要求
cache 故障不得降级到本地状态；这里同理——放行会把一次 cache 故障变成"实例上所有限制
全部失效"，那正是攻击者想要、而运维看不见的时刻。

该还的扣减都会还。某一行拒绝时，会把同一次请求中更早的行已经拿走的全部退回。
`Admitted::finish()` 表示请求确实跑过了：窗口计数因此保留，但并发 permit 仍会归还——
permit 度量的是在途请求数，不是已发生请求数。丢弃一个 lease 会归还其上尚未结清的部分；
由于 cache 是异步的而 `Drop` 不能 await，归还是 spawn 出去的，能够 await 的宿主应改
调用 `Admitted::release()`。每份扣减都随窗口过期，所以即便一次归还丢了，也会在边界处
自愈。

## 只在新建数据库上存在的级联

`api_keys.organization_id` 与 `api_keys.team_id` 声明了 `on_delete = "Cascade"`，
但 SQLite 的增量升级路径用 `ALTER TABLE ADD COLUMN` 加这两列，而这种语句无法带外键。
只有全新创建的数据库才有该约束，升级上来的没有，其他后端未验证。

**因此删除组织或团队时，必须由本 crate 自己解绑或删除受影响的 API key。** 一个预算链、
权限主体和凭证可见性边界都已不存在的 key 不能继续服务请求，而在升级上来的实例上，
这一层以下没有任何东西会拦住它。
