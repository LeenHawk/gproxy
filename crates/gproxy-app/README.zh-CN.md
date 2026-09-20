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
`publication.rs`）要等 `gproxy-sdk` 存在之后才落地；当前本 crate 到快照与配置为止。

## 现在有什么

| 模块 | 内容 |
|---|---|
| `config` | `AppConfig`：纯 serde，两个宿主共用，不碰文件系统也不读环境变量 |
| `snapshot` | `AppData`——某个配置 revision 编译后的身份，以及单调发布它的 `AppSnapshot` |
| `error` | `AppError`，带 `status_code()` 与用于 API 信封的稳定 `code()` |

其余模块（`auth`、`admission`、`call`、`service`、`capture`、`publication`、
`operations`、`audit`、`dto`）都已声明并写明各自将承担的契约，后续阶段就地填充，
不需要再改 crate 的形状。

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

## 只在新建数据库上存在的级联

`api_keys.organization_id` 与 `api_keys.team_id` 声明了 `on_delete = "Cascade"`，
但 SQLite 的增量升级路径用 `ALTER TABLE ADD COLUMN` 加这两列，而这种语句无法带外键。
只有全新创建的数据库才有该约束，升级上来的没有，其他后端未验证。

**因此删除组织或团队时，必须由本 crate 自己解绑或删除受影响的 API key。** 一个预算链、
权限主体和凭证可见性边界都已不存在的 key 不能继续服务请求，而在升级上来的实例上，
这一层以下没有任何东西会拦住它。
