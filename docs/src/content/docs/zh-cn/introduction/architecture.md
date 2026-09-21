---
title: 架构
description: GPROXY v4 的构造——不含 server 的引擎、不含传输层的产品层、三个宿主共用一张路由表，以及让它们彼此分开的接缝。
---

v4 是对 v3 的彻底重写。要做的事一样，结构不一样。本页是地图：crate、它们之间的接缝，
以及不让结构漂回去的那几条规则。

能解释其余一切的一句话：**每一层只回答一类问题，向下交出一个收窄后的答案，下层绝不
重新打开它。**

## 两种装配

```text
sdk  =  引擎 + 配置写入 + 登录 + 解析 + 查询 + 同步
app  =  sdk + 身份与准入 + 管理面／用户面／issuer 的类型化操作
host =  app + 一个传输层
```

`gproxy-sdk` 是可嵌入的句柄。它用默认实现装配出引擎，拥有推进
`settings.config_revision` 的配置写入，把一次登录变成一行凭证，把模型名解析成执行计划，
并让部署里的每个实例停在同一个 revision 上。它**不含 HTTP server，也不含身份**。

`gproxy-app` 补上"谁在调用、能触达什么"：用户、网关 API key、组织、团队、权限、订阅、
限流、OAuth issuer、审计，以及写入其中任何一项的操作。它**不含 server**——没有 router、
没有 runtime、没有 CLI——这正是同一套判断既能跑在原生 axum server 后面、也能跑在
Cloudflare Worker 里的原因。

宿主是剩下那层薄壳：字节进来，类型化调用出去，字节回去。

## crate 清单

| Crate | 装什么 | 绝不 |
| --- | --- | --- |
| `gproxy-protocol` | 连接建模（`WireRequest`、`WireResponse`、body、分帧、套接字）、操作注册表、OpenAI／Claude／Gemini 线类型，以及协议适配所需的宿主能力接口 | 知道渠道存在；匹配 URL 路径 |
| `gproxy-channel` | `BaseChannel`、可选能力 trait，以及按 feature 启用的 25 个上游适配器 | 依赖引擎或 store；选择 client 后端 |
| `gproxy-client` | 出站传输契约及其 `reqwest` / `wreq` 实现，加上 wasm 的 `fetch` 与 Workers 后端 | 读数据库、选凭证、做协议转换 |
| `gproxy-core` | 引擎：Provider 执行快照、允许集合内的凭证选择／刷新／健康、转换、改写、预算、计价、结算与观测 | 解析路由、检查权限、跑 server |
| `gproxy-store` | 表结构与查询，后端按 feature 切 | — |
| `gproxy-cache` | TTL 状态、原子计数／许可／租约、跨实例失效通知；Memory 与 Redis | 业务实体；后端故障时静默降级 |
| `gproxy-seaorm` | SeaORM 批量读写、D1 binding、类型映射、结构同步 | 业务实体 |
| `gproxy-tokenizer` | 字符串 → token 数，基于本地共享词表 | 解析请求、下载、后台任务 |
| `gproxy-file` | 可选的本地或 S3/R2 内容存储，基于 OpenDAL | 文件元数据；归属关系 |
| `gproxy-sdk` | 句柄：装配、`manage()`、`login()`、解析、会话、`call()`、`query()`、`sync` | 任何 HTTP server 代码；任何身份 |
| `gproxy-app` | 身份、准入、两个产品面和 issuer | 传输层 |
| `gproxy-host-axum` | HTTP 路径 → 操作。那张路由表 | 自己的产品判断 |
| `gproxy-host-edge` | `fetch` → 那个 axum router | 第二张路由表 |
| `gproxy-host-tauri` | IPC → 操作，只绑管理面与用户面 | 数据面 |
| `gproxy` | 命令行：环境、文件、socket、终端 | 任何库能做的事 |

箭头只朝一个方向：宿主依赖 app，app 依赖 sdk，sdk 依赖引擎。引擎永远不会长出监听器、
router 或 UI。

## 每个问题在哪里被回答

| 问题 | 谁回答 |
| --- | --- |
| 谁在调用 | `gproxy-app` 的认证器 |
| 能触达什么、花谁的钱 | `gproxy-app` 的准入 |
| 哪个 Provider 承接这个模型名 | sdk 的解析器 |
| 哪把凭证、失败了怎么办 | 引擎 |
| 一个操作*做*什么 | `gproxy-app` 的操作家族与 sdk 的 `manage()` |
| 一次失败在线上值什么状态码 | `AppError::status_code` / `code` |
| 哪个 URL 对应哪个操作 | axum 宿主 |

任何宿主若重新回答前五个中的任何一个，它的答案只会应用在 HTTP 上而不会应用在 IPC 上，
而这正是接缝存在要防的那一类 bug。

## 解析与 Plan

解析发生在**进引擎之前**，所以引擎只见已解析的目标。模型名按第一条命中的规则解析：

| 名字 | 解析为 | 尝试预算 |
| --- | --- | --- |
| 没有模型 | 全部启用的 Provider，无上游模型 | `settings.max_attempts` |
| 公开模型名 | 该路由启用的成员 | 路由自己的 |
| `渠道/模型` | 该渠道的 Provider，优先目录里列了该模型的 | `settings.max_attempts` |
| `Provider 名/模型` | 那一个 Provider | `settings.max_attempts` |
| 其他 | unknown model 错误 | — |

公开名精确匹配，并且**先于**前缀形式，因此运维者可以把字面量 `openai/gpt-5` 作为自己的
公开名暴露出来。

前缀形式内部，**渠道 id 胜过同名 Provider**。渠道 id 由构建固定、改不掉；Provider 名随时
可以改。反过来更糟：有人把 Provider 命名为 `codex`，全部 `codex/*` 流量就再也到不了
`codex` 渠道，而且没有任何绕开的办法。

候选随后按渠道、允许的 Provider id、允许的凭证 id 收窄——最后一项是产品层做组织间隔离的
手段。排序是 `(tier, 健康度, 权重倒序, 稳定 id)`；只有首段按路由策略做均衡。

## 失败转移有两个维度

引擎已经在单个 Provider 的凭证之间重试过了。sdk 是另一个维度，**只有"另一个 Provider
可能救得回来"的失败才换目标**：没有可用凭证、凭证已死、续接钉在别的实例上、任何渠道或
传输错误，以及 401／403／429／5xx 应答。预算耗尽、被禁止、被取消、转换错误，以及 store
或 cache 故障，都就地停止——在别处重试只是多花一点。

尝试预算跨目标共享，因此一个计划的上游调用次数不会超过它的 `max_attempts`。没有目标可换
时，最后一个应答原样返回——最后一个 Provider 的 429 就是调用方的 429，不会被换成别的错误。

## 一次写入 = 一个 revision + 一次重载 + 一次通知

```text
commit_revision([语句…, config_revision += 1, 读回])   一个事务
      ↓
reload   本实例的快照
      ↓
publish  ConfigurationChanged { revision, scopes }
```

行与 revision 自增是同一个事务。拆开就有两个都会真实发生的窗口：行落库而自增没落，
同伴永远不知道该重载，这条配置在别的实例上隐身；或者自增落库而行没落，所有同伴为一份
不存在的变更各重载一次。

**重载必须在通知之前。** 反过来就等于宣告一个自己还服务不了的 revision。通知本身是尽力
而为——cache 拒绝发布只让部署多等一个轮询周期，这正是轮询兜底存在的理由。

两套机制，缺一不可：

| | 传递什么 | 会怎么失败 |
| --- | --- | --- |
| 共享 cache 上的失效通知 | 毫秒级的"再看一眼" | 丢消息 |
| `settings.config_revision` 轮询 | 持久事实，默认 30 秒 | 慢 |

重载串行且单调推进：旧 revision 绝不覆盖新的，重载失败保留上一份快照继续服务。

## 结算不是可选项

引擎保证每一条到达上游的路径都走同一个漏斗：给交换定价、扣减每一个适用的预算窗口、
把报告交给观察者。没有绕过它的快路径，因为绕过它的路径就是未计量流量。

*可选*的东西是**干活之前先问**，而不是事后丢弃结果。关掉的 capture 不分配任何东西、
不复制任何 body；v3 的做法是先把整份成本付掉，再让 sink 决定要不要。

费用要等交换结束才知道，所以预算**最多被超出一个请求**——这是不用预估、不用预扣、
不用回滚所付的代价。结算按请求 id 幂等，结算失败只丢记账，不影响已经交付的响应。

没有价格规则覆盖的模型按 0 结算，并带上 `unpriced = true` 维度。运维者要的是这个信号，
不是一个拒绝。

## wasm 是目标，不是分叉

`gproxy-app` 能为 `wasm32-unknown-unknown` 构建，axum router 也能，这正是
`gproxy-host-edge` 挂载同一个 `Router` 而不是把路由表写第二遍的原因。axum 的 server
那几半是 Cargo feature 而不是 crate 本身；wasm 构建少掉的东西都是 socket 形状的，从来
不是一条路由。

真正的障碍是 `Send`。wasm 上引擎**按设计**是 `!Send`——JS 传输句柄属于创建它的 isolate
——而 axum 要求 state `Send + Sync`、handler future `Send`。两处运行期检查的桥接解决了它，
都成立于"Worker isolate 是单线程"：app 把句柄放进 `SendWrapper`，axum 宿主包住每个
handler 体。漏包的 handler 是 **wasm 目标上一个指名道姓的编译错误**——这就是强制手段，
也是它放在 handler 而不是 router 的 `cfg` 上的原因。**编不过 edge 的路由，不可能在 edge
上悄悄不存在。**

## 不让它漂回去的规则

- 引擎永远不依赖 server 框架、router 或 UI。
- 每一条到达上游的请求都从同一个漏斗出去。
- 转换是成对的；没有中间表示，转换器也不读写未知字段袋。
- 一次配置写入就是一个事务加它的 revision 自增。
- 快照发布是单调的；重载绝不倒退。
- 装配绝不因为一行坏数据而失败——丢掉、计数、记日志。
- edge 宿主没有自己的路由表。
- 前端类型由 Rust 生成，绝不手写。
