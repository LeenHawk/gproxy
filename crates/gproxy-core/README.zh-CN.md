# gproxy-core

[English](README.md) | 简体中文

不含下游职责的 Provider 执行引擎。上层完成路由与模型别名解析、调用方认证、策略与
准入，然后把 Store、Cache、Observer 和一个 `ExecutionTarget`（Provider、上游模型、
允许的凭证集合）交给 core。core 负责协议转换、请求／响应改写、凭证选择／刷新／可用
性、HTTP 与 WebSocket 上游调用、流式转发、观测与结算、多次调用适配所需的续接状态与
资源、agent 会话 assignment、账号额度观测、用量抽取和本地 token 估算。

| 模块 | 职责 |
|---|---|
| `builder` | `Core::builder(store)`：cache、observer、secret codec 必填；渠道注册进 `ChannelRegistry`；可选 client 池与文件存储 |
| `data` / `runtime` / `context` | 执行快照、原子凭证材料、block 与窗口 key、已解析目标与请求／attempt／exchange 上下文、用量报告 |
| `secret` | `SecretCodec`：默认 `AesGcmCodec`（AES-256-GCM 信封，每凭证数据密钥，凭证 ID 进 AAD），`PlaintextCodec` 需显式选择 |
| `limits` | 由 Setting 行得到 `ExecutionLimits`，派生所有 `CapabilityLimits` 与 `CodecLimits`；没有无限模式 |
| `assemble` | ControlData 行装配成 `CoreData`：profile 到池化 client、渠道查找、开秘、endpoint 校验、规则编译、`QuotaModel` 维度、自定义词表、存活 block |
| `rewrite` | 规则编译、按阶段／操作／模型／头选择、Body/Header/Query 应用、保留原字节的逐单元流改写（SSE、JSON 数组、NDJSON） |
| `select` / `availability` | 允许集合内按策略、亲和与 block 选凭证；失败 streak 与冷却 |
| `execute`（私有） | HTTP 与 WebSocket attempt 循环、观测包装的 client／body／socket、请求缓冲、结算漏斗与 `Settled` 证明 |
| `convert` | 直通或转换的路由、原生端点、每个协议族一个驱动，在 attempt 绑定的 upstream 上运行 protocol 的适配流程 |
| `capability` | core 的 `AttemptUpstream`、`ProtocolState`、`Resources`，即 protocol 适配所依赖的宿主能力 |
| `refresh` | 显式凭证刷新：跨实例租约、渠道 `CredentialRefresh`、密封、版本 CAS、发布，确定性拒绝写 `Dead` |
| `quota` | `QuotaHeaders`／`QuotaQuery` 观测写入 `credential_quota_cycles` 与耗尽 block；Counted 维度落 Store `counted_windows` 行计数 |
| `session` | agent 会话 assignment：可用时保持绑定，持久性失效时预留新代，由准备它的 attempt 激活或标记失败 |
| `estimate` | 上游未计量的交换的本地 token 估算 |
| `observe` | 宿主的结算／capture／trace 漏斗，先问 policy 再做事 |
| `keys` | 唯一的 cache key 语法与失效通知 topic |

## 快照与数据

`CoreData` 保存 Provider、凭证、共享改写规则集、执行限额和估算器。不保存路由、对外
模型别名、身份表、权限、OAuth client 白名单、订阅、计价或准入规则；这些连同路由亲和
与跨 Provider 均衡都属于上层。`publish_snapshot` 只接受更新的 revision；请求各自
持有 `Arc`，重载不会中途改变请求。

`ExecutionTarget` 指定一个 Provider、可选的上游模型和确切的允许凭证集合。core 只在
该集合内轮换与重试；空集合是"没有可用凭证"，绝不是全局池。上层给出 attempt 预算、
截止时间和不透明的隔离 scope。core 不解释用户或 key 的角色。已绑定的远端资源随其
原始目标与受限凭证一起传入。

`CredentialData` 带有解析好的 HTTP client、强制 HTTP/1.1 用于 WebSocket 升级的同一
profile、渠道为该凭证声明的 `QuotaDimension`，以及共享的 `CredentialState`：秘密、
到期、生命周期状态与 Store 版本作为一个 `CredentialVersion` 原子发布，每个 attempt
固定一个版本，只有更新的版本才能替换。`Dead` 凭证永不被选择或刷新；`status_reason`
说明原因，`CoreError::CredentialDead` 告诉调用方需要有人重新登录。

可用性是每凭证一份 `CredentialBlocks` cache 载荷，加载时从 `credential_blocks` 行重建。
每个 block 指定渠道 `QuotaScope`、可选操作、`until_ms` 和 `BlockSource`：Reported 维度
观测到耗尽（带持久化的周期记录）、Counted 窗口用尽、上游限速、连续失败。streak 按
scope 与操作分别记录，坏掉的模型既不藏在健康模型后面，也不拖累它。见
[凭证可用性](../../design/core-credential-availability.md)。

`CredentialStrategy` 为 `RoundRobin`、`Sticky` 或 `RoundRobinAffinity`
（`round_robin_affinity`）。Sticky 与亲和遵循成功 attempt 后写入的会话 pin；未绑定的
请求推进按候选集合分键的共享轮转计数器。pin 按调用方 scope 与 Provider 隔离。

## 公开 API

[api/operations.rs](src/api/operations.rs) 声明与 `BaseChannel` 对应的 30 个具名 HTTP
方法和 3 个具名 WebSocket 方法，以及通用的 `send`／`connect`。具名方法拒绝不匹配的
context 操作。每个方法返回 `Execution<T>`：protocol 响应或连接，加上请求结算时解析的
`UsageCompletion`。

[api/lifecycle.rs](src/api/lifecycle.rs) 是 Store／账号面：

| 方法 | 行为 |
|---|---|
| `load_data` / `reload_data` | 一次 `load_control_data` 批读、装配、存活 block 预热进 cache、自定义词表从文件存储解析一次；单调发布，加载失败保留旧快照 |
| `reload_credentials` | 按输入顺序重读行并发布到既有槽位；缺失行返回 None 并摘除槽位 |
| `refresh_credential` | Provider 归属校验、每凭证 cache 租约（其他实例等待，更新的持久版本可直接满足调用）、权威 Store 读取、渠道刷新、密封、`refresh_many` CAS、发布并发出 `CredentialChanged` 通知；`RefreshRejected` 写入带原因的 `Dead` |
| `query_credential_quota` | 通过指派 client 调用渠道 `QuotaQuery`；每条 entry 写一行 `credential_quota_cycles`，已声明维度耗尽则打 block |

## 执行

每个 attempt 在允许集合内选凭证（enabled、Active、未摘除、对该模型与操作未被 block），
对一分钟内到期的材料先刷新，在发出前对 Counted 请求维度计数，准备请求并经
`ChannelBinding` 用 Provider 的方法 URL 分派。流式请求体缓冲到限额以便重试重放；超限
则降为单次尝试。

回答按状态分类：2xx 与客户端 4xx 直接返回；429 写 `RateLimited` block（Retry-After
或 30s），若渠道 `QuotaHeaders` 报告了耗尽的维度，则改写 `QuotaExhausted` block 直到
上游 reset；可刷新凭证的 401/403 强制刷新一次后重试同一凭证；5xx 与传输错误累加该
scope 的失败 streak 并重试，到三次写冷却 block。预算或候选耗尽时返回最后一个上游
回答。每个回答的头都交给 `QuotaHeaders`，成功时同样观测账号额度。

交回的每个响应 body 都是一条观测流：capture、用量观测、逐单元响应改写、读取上限、
空闲超时与取消；结束或丢弃它恰好结算请求一次，且用量在任何结算前先记录。
`Execution` 只能凭漏斗的 `Settled` 证明构造。WebSocket 操作以同一循环完成握手；建立
的 socket 双向 capture、计量与改写，结束或丢弃时结算。

## 转换

`convert::route` 询问渠道该 Provider 对此操作原生支持哪些 dialect（`action = "dialects"`
的 `OperationRule` 可覆盖）：客户端 dialect 原生则直通，否则第一个声明的 dialect 为
转换目标。流式客户端对只有缓冲生成的上游走 `Route::Synthesize`：一次缓冲上游调用，
再由结果回放客户端的原生流生命周期。

转换在 `AttemptUpstream` 上运行 protocol 的适配流程，流程内每次原生调用都像直通一样被
capture、计量与改写，原生的拒绝回答重新进入同一分类。

| 族 | 覆盖 |
|---|---|
| Generate | 全部十二个 dialect 对，缓冲与流式；Chat `n`／Gemini `candidateCount` 拆成有日志的子调用；Chat、Claude、Gemini 客户端打到 Responses WebSocket 上游；Responses WebSocket 客户端逐 turn 由 Chat、Claude 或 Gemini HTTP 上游承接 |
| Models | 列表与单个，OpenAI、Claude、Gemini 之间借助 Provider 模型 supplements |
| Count tokens | Claude 与 Gemini 目标；OpenAI 作为目标刻意不支持 |
| Embeddings | OpenAI ↔ Gemini，单条与批量 |
| Guardian、compact、memory | 四种目标任一；guardian 流式拒绝 |
| Files | 获取、列表、删除、内容与 multipart 上传，含 Gemini 可续传协议 |
| Images | OpenAI create/edit 经 Gemini 或 Responses 图像工具；URL 交付拒绝 |
| Video | OpenAI 原生视频经 Veo，任务状态存 `ProtocolState` |

## 凭证生命周期、额度与会话

`refresh_credential` 是显式账号操作；attempt 循环在固定即将到期的材料前以 `IfNeeded`
调用，401/403 后以 `Force` 调用。额度分两条线：Reported 维度的值来自头、查询或耗尽
回复，block 到上游周期结束（或按维度推一个窗口）；Counted 维度落 Store 的 `counted_windows`
行，用"装得下才加"的原子更新计数，请求在交换前（被拒绝就不发请求换凭证）、token 在用量
结算后，block 到窗口结束；各实例看同一个数，重启不丢。对不上
任何声明维度的 entry 仍持久化为周期记录。

`SessionIdentity` 指向 `agent_sessions` 行的请求是 agent 会话：core 在可用时把它保持在
激活 assignment 的凭证上，瞬时问题（限速、冷却、本次请求的 5xx）时不绑定地走另一凭证，
只有凭证持久性失效（dead、禁用、摘除、额度或计数 block）才预留新代，并带上耗尽周期与
触发请求作为证据。准备它的 attempt 在上游任何回答时激活预留，否则标记失败；并发的
首次请求收敛到同一个待定预留。

## 用量与估算

`NormalizedUsage` 复用 gproxy-channel：一个请求可有多个 attempt，每个 attempt 有多次物理
交换，按 capture ID 各报告一次。上游已服务但未报告用量（或缺 token 数）的交换，在
Setting 开启用量时本地估算：输入 token 由请求的提示文本按上游模型的 tokenizer（目录
词表文件、Setting 默认、或内置编码器）计数，输出 token 取响应字节的一半。提示文本通过
交换原生 `(operation, dialect)` 对的协议 wire 类型读取——系统提示、消息、工具调用与结果、
工具声明——覆盖 Claude Messages、OpenAI Chat、OpenAI Responses（HTTP 与 websocket 信封）、
OpenAI count-tokens、guardian、compaction、memory 与 embeddings，以及 Gemini generate、
count-tokens 与 embeddings。base64 媒体、文件 id、URL 与加密数据一律不计。没有按键名
兜底：没有提取器的对，或 wire 类型拒绝的请求体，完全不做输入估算（输出估算仍然生效）。
估算标记为 `Partial` 并带 `dimensions["estimated"] = "true"`；已报告的值绝不覆盖，
被拒绝的回答不估算。`UsageState::Skipped` 表示请求策略关闭了用量，区别于上游没有报告。

## 观测

[observe.rs](src/observe.rs) 是宿主的漏斗。`Observer::policy` 在任何 attempt 前每请求
回答一次；关闭的工作不会做了再丢。`capture` 为每次物理交换开一个 `CaptureSink`；
`usage` 在开启用量时每请求恰好调用一次，包括取消与失败；`trace` 收到借用的 attempt、
exchange 与刷新事件。记录不会改写、重排或延迟交付的流。

## protocol 能力

| 类型 | trait | 绑定 |
|---|---|---|
| `AttemptUpstream` | `Upstream<Target = OperationKey>` | 一次 attempt 的 Provider、固定的凭证版本与 client；自持且可克隆，惰性启动的 fanout 子调用可超出 attempt 栈帧存活 |
| `ProtocolState` | `StateStore<Scope = StateScope>` | Store 的 ProtocolState 行；scope 是调用方 scope、Provider 与可选会话 |
| `Resources` | `ResourceAccess<Scope = ResourceScope>` | 经文件存储落 `resource_bindings` 与 `file_objects` 的发布；`Id` 先解析同 scope 的发布，再走 scope Provider 的 files API；`Url` 不支持 |
| `ChannelStateStore` | `gproxy_channel::ChannelState` | 同一批 ProtocolState 行，限定到一个 Provider 与一个凭证，作为 `OperationContext.state` 交给渠道；渠道只选 key，看不到 scope |

每次渠道调用还带 `OperationContext.instance_id`，即宿主进程的身份（`CoreBuilder::instance_id`，
缺省随机）。必须在两次请求之间保持上游活连接的渠道（claudeweb 在 `tool_use` 处停放
completion 流）把持有者记在续接记录里。之后的请求落到另一个进程时，attempt 以
`CoreError::ContinuationElsewhere { instance_id }` 失败：不重试、不给凭证记失败、续接原地
保留。多实例宿主要么把会话转发到那个实例，要么一开始就把会话粘住；单实例永远不会看到它。

## wasm32

引擎在 wasm32-unknown-unknown 上以同一套 API 运行：定时器与后台任务来自 JS 事件循环，
出站传输来自 gproxy-client 的 `fetch`／`workers` feature 或宿主注入的 `OutboundClient`
（见 client README），文件存储来自 gproxy-file 的 `s3` feature（R2 或任何 S3 兼容存储，
经 fetch；本地文件系统后端只在原生），估算用同一套 tokenizer。内置的 DeepSeek 词表是
`bundled-vocabulary` feature（默认开启，约 4 MiB 进二进制）；关闭后没有目录词表文件的
模型退回字符估算。测试只在原生运行。

```sh
cargo test -p gproxy-core
cargo clippy -p gproxy-core --all-targets -- -D warnings
cargo clippy -p gproxy-core --target wasm32-unknown-unknown --all-targets -- -D warnings
```

测试基于内存 SQLite Store、内存 cache 与脚本化渠道／client；它们证明引擎的契约，不代表
可用的网关或真实供应商。见 [crate 边界](../../design/crates.md)。
