# gproxy-core

[English](README.md) | 简体中文

Provider 执行层的数据结构。上层先完成路由／模型别名解析、身份认证、策略判断和准入，
再调用 core。当前完成结构及进程内单调发布，尚未接入执行器和凭证选择器。

| 模块 | 数据结构 |
|---|---|
| `data` | Provider／Credential 执行快照、channel/client 引用、预编译改写匹配器 |
| `runtime` | 原子凭证版本、凭证亲和／轮换进度、按范围的可用性 block、Counted 维度计数 key、执行数据失效通知 |
| `context` | 已解析执行目标、不透明调用方 scope／session、请求／尝试／交互、用量报告 |
| `observe` | 宿主实现的结算／capture／trace 漏斗，先问策略再干活 |
| `capability` | protocol 的 Upstream／StateStore／ResourceAccess 在 core 侧的实现，绑定 attempt 或 scope |
| `assemble` | ControlData 行装配成 CoreData：profile 解析成 client、渠道查找、解密、规则编译、额度维度、在效 block |
| `rewrite` | 规则编译（正则、点路径、筛选），执行改写设计里的 target／phase／name 校验 |
| `keys` | core 所有读写方共用的唯一一套 cache key 语法 |

`CoreData` 只保存 providers、credentials、可复用 rewrite_rule_sets。路由、对外模型别名、
身份表、权限、OAuth client 白名单、订阅、计价和准入规则属于上层，路由亲和以及跨 Provider
负载均衡／失败转移也由上层完成。

`ExecutionTarget` 由上层传入单个 Provider、可选的已解析上游模型和明确允许的凭证集合。
后续执行器必须仅在该集合内轮换／重试；空集合不能回退为全局凭证池。上层同时给出最终
尝试预算和完成鉴权的不透明隔离 scope；core 不解析用户／key 权限。已绑定远程资源应传入
原目标和受限凭证，assignment 关联信息仅作归属记录，不代表可以迁移资源。

新增 `CredentialStrategy::RoundRobinAffinity`，序列化为 `round_robin_affinity`。
新会话／未绑定会话按轮询分配凭证，后续请求复用同一份仍可用且被允许的凭证，命中绑定
不推进轮询游标。绑定缺失、过期或原凭证不可用／不再被允许时，在给定集合内重新分配。
没有稳定会话标识时退化为普通逐请求轮询。绑定按调用方隔离 scope 和 Provider 区分；
同一会话并发首次请求需要收敛到同一个绑定。目前只定义策略契约，持久化及选择器尚未实现。

`Core<C>` 通过 `Core::builder(store)` 装配：`cache`、`observer`、`secret_codec` 必填，
`channel`／`channels` 把 `BaseChannel` 实现注册进 `ChannelRegistry`（重复 ID 拒绝），
`client_pool` 可覆盖 core 自持的 `gproxy_client::ClientPool`。构造不做 I/O，没有任何
默认的空实现：不想结算、不想加密的宿主必须显式声明。`SecretCodec` 负责凭证 secret 的
落库密封：`AesGcmCodec`（AES-256-GCM 信封，每凭证数据密钥由主密钥包裹，AAD 绑定凭证 ID）
是默认选择，`PlaintextCodec` 是显式放弃加密；两者互不接受对方的信封。`ExecutionLimits`
来自 Setting 行（请求／流空闲超时、body／事件／帧字节上限、multipart 分片数），派生所有
`CapabilityLimits` 与 `CodecLimits`；没有无限模式，请求 deadline 只能收紧。
`publish_snapshot` 只接受更新 revision 的已验证执行快照；请求持有自己的
Arc。执行数据组装由 core 的加载／重载入口负责；持久 revision 分配及通知协调由上层接线。

`CredentialData` 保存执行配置、已解析的 HTTP client、同一 profile 强制 HTTP/1.1 的
WebSocket client，以及共享 `CredentialState`。解密 secret、到期时间、生命周期 status 和
Store version 作为一份 `CredentialVersion` 整体原子发布，每次尝试固定一份版本。仅在 Store CAS 成功或
权威读取后发布，拒绝相同／旧版本。仍存在的凭证复用 slot，删除后摘除，重新创建用新 slot。
发布方法不隐式获取刷新租约或写库，刷新不需要替换整个执行配置快照。

凭证可用性是每个凭证一份 `CredentialBlocks` cache 载荷，不是配额账本；每条 block 同时是
Store `credential_blocks` 的一行，"部分受限"跨重启保留，加载时从 Store 重建 cache。每条
`CredentialBlock` 带渠道 `QuotaScope`（全部／模型列表／模型族前缀）、可选操作、`until_ms`
和 `BlockSource`：Reported 维度观测到耗尽（关联已落库 `CredentialQuotaCycle`）、Counted
维度窗口用完、限速、连续失败。连续失败按 scope／操作分别记为 `FailureStreak`，坏掉的模型
既不会被健康模型掩盖，也不会拖累它。选凭证时调 `blocked_by(model, operation, now)`；没有模型
的请求只受凭证级 block 影响，`Unknown` 范围 block 整个凭证。core 不把模型族前缀展开成
模型列表。持久生命周期另算：`CredentialData.status` 对应 Store 列，`Dead` 不选也不刷新，
`status_reason` 说明原因。刷新在确定性拒绝时写 `Dead`，`CoreError::CredentialDead` 告诉
调用方需要人来重新登录。`CredentialData.quota` 是组装时渠道 `QuotaModel` 为该凭证声明的
`QuotaDimension` 列表；Counted 维度在 cache 里按 `CountedWindowKey` 计数。剩余额度本身
留在渠道 `QuotaSnapshot` 和 Store 周期记录；见[凭证可用性设计](../../design/core-credential-availability.md)。

CoreData 和解密凭证不实现 Debug／Serialize。session 由入站层提供，RequestFallback
不具有跨请求稳定性。请求、响应和流继续复用 protocol 类型；用量复用 channel 的
`NormalizedUsage`。一个请求可有多次尝试，一次尝试可产生多次真实交互；capture ID 属于
交互，不属于流片段。`UsageReport` 同一份值既送入宿主 Observer 漏斗，也解析给调用方的
`UsageCompletion`；不携带费用计算、订阅分摊或准入预占规则。`UsageState::Skipped` 表示
本请求策略关闭了用量观测，与上游没有报告用量是两回事。

`RewriteTarget` 分为 Body（持有可选 JSON 路径）、Header（已校验的 HeaderName）和
Query（解码后的参数名称）。Header／Query 面向重复字段值，Query 仅限 request；
编译校验和实际修改尚未接入，见[改写设计](../../design/core-rewrite.md)。

ProviderData 增加 `operation_urls`，按目标原生 `OperationKey`＋HTTP/WS 区分；
`operation_url` 返回从 Store OperationEndpoint 组装的启用地址，None 表示沿用
Provider／渠道默认值。实际 channel 调用时应用该地址仍需后续接线。

## Public API

[api/operations.rs](src/api/operations.rs) 包含与 BaseChannel 对齐的 **30 个 HTTP 命名方法、
3 个 WS 命名方法**，并保留 `send`／`connect` 统一分派。命名方法会拒绝 context.operation
不匹配的调用，不偷偷替换上层已经准入的操作。HTTP 分派对 Operation 穷尽匹配，新增
协议操作时编译器会要求补齐入口。请求／响应／流仍直接使用 protocol 类型。

| 分组 | 命名入口 |
|---|---|
| 模型／计数 | `list_models`、`get_model`、`count_tokens` |
| 生成 | `generate_content`、`stream_generate_content` |
| 审查／上下文 | `guardian_review`、`guardian_classify`、`compact_content`、`summarize_memory`、`create_conversation` |
| 向量／检索 | `create_embedding`、`batch_create_embedding`、`rerank`、`web_search` |
| 图像／音频 | `create_image`、`edit_image`、`create_speech`、`create_transcription`、`create_translation` |
| 文件 | `create_file`、`list_files`、`retrieve_file`、`retrieve_file_content`、`delete_file` |
| 视频 | `create_video`、`retrieve_video`、`list_videos`、`delete_video`、`download_video_content` |
| Realtime／WS | `create_realtime_call`（HTTP）、`connect_realtime`、`generate_content_websocket`、`stream_generate_content_websocket` |

[api/lifecycle.rs](src/api/lifecycle.rs) 提供 Store／账号能力入口：

| 方法 | 契约 |
|---|---|
| `load_data` | 已实现：一次 `load_control_data` → `assemble`（启用的 provider 找注册渠道、凭证→Provider→Setting 的 profile 解析成池化 client、解密、endpoint 校验、规则编译、`QuotaModel` 声明维度、读 `config_revision` 与 limits）→ 在效 `credential_blocks` 预热进 cache；任一行无效整次失败 |
| `reload_data` | 已实现：加载后按 revision 单调发布，加载失败保留旧快照 |
| `reload_credentials` | 已实现：按输入顺序重读，材料发布进已有 `CredentialState` slot，缺失行返回 None 并摘除 slot；上次 reload 之后新建的凭证只报告不安装，需要 `reload_data` |
| `refresh_credential` | 显式 Provider／凭证 ID＋IfNeeded/Force；租约、当前 Store 读取、channel 刷新、密封/CAS 持久化、发布 |
| `query_credential_quota` | 调用指定凭证的上游额度能力，不做订阅聚合或额度重置 |

凭证入口返回不含 secret 的 `CredentialSummary`（版本、到期、持久 `CredentialStatus`）或现有 channel `QuotaSnapshot`，由可信上层
授权这些 ID。`load_data` 只加载执行相关数据，不读路由、身份、权限、订阅、价格表；也不
负责打开数据库或 sync。组装所需 channel/client 注册、secret codec 和持久 revision 接线
仍待实现。

实际 HTTP／WS 执行、凭证刷新及额度查询仍返回 `CoreError::NotImplemented`；wasm32 上
`load_data` 也是，因为那里还没有出站传输。不伪造上游调用。`Execution<T>` 保留原始响应和后续
`UsageCompletion`，流生命周期尚未接入。

已移除对外占位函数：`select_credential`、`compile_rewrite_rule`、`rewrite_request`、
`rewrite_response`、`record_outcome`。选凭证、attempt 准备、改写、健康／亲和更新、续接／
资源状态和 capture 写入随内部实现加入，不另加 public 流水线 hook 或重复 CRUD。
`AttemptOutcome` 只作为送给 Observer 的 trace 数据保留。取消和
超时复用 RequestContext 已有 token/deadline，不加尚无后台任务支撑的 shutdown 原型。
路由、policy、OAuth 登录、计价结算和管理 CRUD 继续属于上层。

## 观测

[observe.rs](src/observe.rs) 是 [crate 设计](../../design/crates.md)里结算／capture／遥测
三个开关背后的扩展点：core 保证每条执行路径都走到这里，宿主负责落库、脱敏、计价和
保留期。契约是"先问再干"：`Observer::policy` 在任何 attempt 之前按请求回答一次，关掉的
项目不会先做再丢弃。

| 项目 | 契约 |
|---|---|
| `ObservationPolicy` | `usage`、`capture`（`Off`／`Metadata`／`Full`）、`trace`，按不透明 scope、Provider 和操作决定；没有 Default |
| `Observer::capture` | capture 开启时，每次物理交互打开一个 `CaptureSink`；事件借用 head、chunk 和 WS frame，两个方向统一编号 |
| `Observer::usage` | 每个开启用量的请求恰好调用一次，包括取消和失败的请求，在流／socket 结束之后 |
| `Observer::trace` | 借用形式的 `TraceEvent`：attempt／exchange 开始与结束、凭证刷新；trace 关闭时不格式化任何字符串 |

记录不改写、不重排、不延迟已交付的流；sink 失败是宿主的问题，不让请求失败。
下游（客户端↔网关）capture 及其与上游交互的关联留在宿主，用 request／attempt／capture
ID 对应。core 目前还没有调用这些方法。

## protocol 宿主能力

[capability.rs](src/capability.rs) 实现 `gproxy-protocol` 适配流程泛型依赖的宿主能力
trait，多次调用的适配不会拿到裸 client：

| 类型 | Trait | 绑定 |
|---|---|---|
| `AttemptUpstream` | `Upstream<Target = OperationKey>` | 一次 attempt 的 Provider、固定凭证版本和 client；target 只是要分派的原生操作 |
| `ProtocolState` | `StateStore<Scope = StateScope>` | Store 的 ProtocolState 行；scope 为调用方 scope＋Provider＋可选会话 key，由 core 序列化 |
| `Resources` | `ResourceAccess<Scope = ResourceScope, PublishedHandle = PublishedHandle>` | Store 的 ResourceBinding／FileObject 行和文件后端；scope 携带允许的 `ExecutionTarget`，来源 Provider 可以不同于本次请求目标 |

每个实例由构造它的执行路径显式传入 `CapabilityLimits`，没有无限默认值。
`AttemptUpstream::send/connect` 需应用 Provider 的方法 URL、拒绝绝对 URL 和来源鉴权，
并经过观测包装层。当前所有方法返回 kind 为 `Unsupported`、说明函数未实现的
`CapabilityError`。

```sh
cargo test -p gproxy-core
cargo clippy -p gproxy-core --all-targets -- -D warnings
cargo clippy -p gproxy-core --target wasm32-unknown-unknown --all-targets -- -D warnings
```

测试覆盖并发发布及保留在途视图，不表示选择器或网关已跑通。边界见
[crate 设计](../../design/crates.md)。
