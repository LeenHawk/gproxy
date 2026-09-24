# gproxy-core

[English](README.md) | 简体中文

不含下游职责的 Provider 执行引擎。上层完成路由与模型别名解析、调用方认证、策略与
准入，然后把 Store、Cache 和一个 `ExecutionTarget`（Provider、上游模型、
允许的凭证集合）交给 core。core 负责协议转换、请求／响应改写、凭证选择／刷新／可用
性、HTTP 与 WebSocket 上游调用、流式转发、观测与结算、多次调用适配所需的续接状态与
资源、agent 会话 assignment、账号额度观测、用量抽取和本地 token 估算。

| 模块 | 职责 |
|---|---|
| `builder` | `Core::builder(store)`：cache、secret codec 必填；默认使用 Store 落库 observer，可显式替换；渠道注册进 `ChannelRegistry`；可选 client 池与文件存储 |
| `data` / `runtime` / `context` | 执行快照、原子凭证材料、block 与窗口 key、已解析目标与请求／attempt／exchange 上下文、用量报告 |
| `secret` | `SecretCodec`：默认 `AesGcmCodec`（AES-256-GCM 信封，每凭证数据密钥，凭证 ID 进 AAD），`PlaintextCodec` 需显式选择 |
| `limits` | 供库调用者显式指定的限额；网关默认不添加大小、数量或时间预算 |
| `assemble` | ControlData 行装配成 `CoreData`：profile 到池化 client、渠道查找、开秘、endpoint 校验、规则编译、`QuotaModel` 维度、自定义词表、存活 block |
| `rewrite` | 规则编译、按阶段／操作／模型／头选择、Body/Header/Query 应用、保留原字节的逐单元流改写（SSE、JSON 数组、NDJSON） |
| `select` / `availability` | 允许集合内按策略、亲和与 block 选凭证；失败 streak 与冷却 |
| `execute`（私有） | HTTP 与 WebSocket attempt 循环、观测包装的 client／body／socket、请求缓冲、结算漏斗与 `Settled` 证明 |
| `convert` | 直通或转换的路由、原生端点、每个协议族一个驱动，在 attempt 绑定的 upstream 上运行 protocol 的适配流程 |
| `capability` | core 的 `AttemptUpstream`、`ProtocolState`、`Resources`，即 protocol 适配所依赖的宿主能力 |
| `refresh` | 显式凭证刷新：跨实例租约、渠道 `CredentialRefresh`、密封、版本 CAS、发布，确定性拒绝写 `Dead` |
| `quota` | `QuotaHeaders`／`QuotaQuery` 观测写入 `credential_quota_cycles` 与耗尽 block；Counted 维度落 Store `counted_windows` 行计数 |
| `budget` | 调用方 USD 预算：宿主指定 owner，懒开 `quota_windows`，attempt 前拒绝、结算、手动重置与状态 |
| `pricing` | `PriceBook`：每快照编译 Store 的价格规则、费率与档位；每次交换在结算时定价 |
| `session` | agent 会话 assignment：可用时保持绑定，持久性失效时预留新代，由准备它的 attempt 激活或标记失败 |
| `estimate` | 上游未计量的交换的本地 token 估算 |
| `observe` | 宿主的结算／capture／trace 漏斗，先问 policy 再做事 |
| `keys` | 唯一的 cache key 语法与失效通知 topic |

## 快照与数据

`CoreData` 保存 Provider、凭证、共享改写规则集、执行限额、估算器、启用的调用方预算和
价格本。不保存路由、对外模型别名、身份表、权限、OAuth client 白名单、订阅或准入规则；
这些连同路由亲和
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

client 按凭证解析，先命中者胜、整份替换不逐字段合并：凭证的 `connection_profile_id` →
Provider 的 → Setting 的默认 profile → 渠道的 `BaseChannel::default_connection_for(Request)` → `ConnectionConfig::default()`。引用的 profile 缺失或
非法时整次装配失败。

Codex 默认使用 reqwest；Claude Code 常规请求使用不带伪装的 wreq。Claude Code 的
Cookie 登录及 Cookie 重新换取令牌、Claude Web 使用浏览器伪装的 wreq。显式设置的
凭证、供应商、全局连接配置仍优先。

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
| `call_service` / `connect_service` | [service.rs](src/service.rs)：按 `ServiceView` 调用渠道的 `ChannelServices`（没有 `OperationKey` 的厂商 CLI 接口）。`Caller`（任何角色）只用网关对显式 `ServiceRequest::user_id` 的记账和绑定到 `scope` 的资源渲染调用方自己的画面——Member 永远看不到任何凭证的状态；`Pool`（仅 admin）把 target 内的凭证合成为一个账号；`Credential(id)`（仅 admin）用该凭证自己的鉴权原样转发。core 不判断谁是谁的管理员：宿主通过选择 `target.credentials` 表达组织边界，`CallerRole::Admin` 指对这个集合的管理员。core 提供事实来源（`TargetCaller`：按显式 user_id 的 usage 行、`ServiceRequest::budgets` 指定的预算的当前窗口、target 凭证的额度周期、`resource_bindings`），取第一个可用凭证（或指名的那条），单项资源路由沿用绑定记录的凭证。角色不符返回 `Forbidden`，渠道没有 services 返回 `Channel(UnsupportedService)`。运行在**漏斗之外**：没有 attempt、usage、capture 和重试 |

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

交回的每个响应 body 都是一条观测流：capture、用量观测、逐单元响应改写与取消；结束或丢弃它恰好结算请求一次，且用量在任何结算前先记录。
`Execution` 只能凭漏斗的 `Settled` 证明构造。WebSocket 操作以同一循环完成握手；建立
的 socket 双向 capture、计量与改写，结束或丢弃时结算。

## 转换

`convert::route` 按 `(操作, 入站协议)` 确定处理方式：直通、转换到指定 `OperationKey`、
本地执行或不支持。渠道声明提供默认值；供应商的 `action = "routing"` 配置以入站协议
为键保存明确映射，修改或恢复一个映射不会覆盖其他入站协议。旧 `dialects` 配置仅作为
支持集合读取，不再解释为优先级。渠道可以声明明确的转换目标；未声明时仅采用唯一可用
的转换组合，存在歧义则保持不支持。

本地处理器从已配置模型目录返回模型信息，并通过选定 tokenizer 统计请求文本，不发送
上游请求。渠道自身的本地模型方法保留其基于凭证的实现。流式生成到非流式生成的明确
映射会从完整响应合成客户端流。

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
| Images | OpenAI create/edit 经 Gemini 或 Responses 图像工具；URL 交付经宿主的 `PublicationUrl`，未配置时在任何调用前拒绝 |
| Video | OpenAI 原生视频经 Veo，任务状态存 `ProtocolState` |

## 凭证生命周期、额度与会话

`refresh_credential` 是显式账号操作；attempt 循环在固定即将到期的材料前以 `IfNeeded`
调用，401/403 后以 `Force` 调用。额度分两条线：Reported 维度的值来自头、查询或耗尽
回复，block 到上游周期结束（或按维度推一个窗口）；Counted 维度落 Store 的 `counted_windows`
行，用"装得下才加"的原子更新计数，请求在交换前（被拒绝就不发请求换凭证）、token 与
定价后的 USD 费用在用量结算后，block 到窗口结束；各实例看同一个数，重启不丢。对不上
任何声明维度的 entry 仍持久化为周期记录。

渠道不知道的上限由运营用 `quotas` 行配置：owner 为 `credential`（`owner_id` 是凭证 id）
或 `provider`（`owner_id` 是 provider id，覆盖该 provider 的每把凭证，除非某把凭证有
`window_key` 相同的自己的行）；`metric` 为 `requests`（unit `count`）或 `cost`（unit `USD`）；
`period` 为 `5h`／`1d`／`7d`（按 `anchor_at_ms` 对齐的固定窗口，未设则 epoch 0）、`1m`
（UTC 自然月）或 `total`（永不重置）；可选 `model_pattern` glob。组装时在渠道自己的维度
之后为每把凭证追加 Counted 维度 `limit:{quota_id}`，执行就是上面那套：请求在 attempt 前
记账，费用在结算后按定价记账（未定价记 0），窗口满了写 `Counted` block 到窗口结束。
字面模型名成为维度的 `Models` scope；glob 在记账时过滤，block 则只写本次模型。
不合法的行告警跳过。这些是上游侧上限，不是调用方预算：不进 `RequestContext::budgets`，
也不结算。`Core::credential_limit_status(credential_id, now)` 报告每条上限当前窗口的
`used`／`limit`；`Core::reset_credential_limit(quota_id, now)` 把行的 `anchor_at_ms` 设为
`now`，删掉该维度的计数窗口，从 Store 与 cache 清掉它的 `Counted` block，再重载受影响的
凭证，计数立即重新开始（固定窗口在重载快照发布后按新 anchor 对齐；`total` 从头开一个
永久窗口）。

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

## 预算与计价

预算是 metric 为 `cost`、unit 为 `USD` 的 `quotas` 行，owner 是一个
`(owner_kind, owner_id)`。kind 由宿主定义（建议 `user`／`api_key`／`subscription`／
`pool`／`team`／`org`，宿主组织里的任何层级都行）；core 逐字匹配，不认识层级关系。
宿主把本次请求花钱的 owner 链以 `BudgetOwner { kind, id }` 放进 `RequestContext::budgets`
（服务 `Caller` 视图用 `ServiceRequest::budgets`），请求扣整条链：个人 key 传
`[api_key:k1, user:u]`；团队 key 传 `[api_key:k2, user:u, team:t, org:o]`，四级里任一
耗尽就拒绝并报那条额度；同一用户在另一个组织的 key 传 `[api_key:k3, user:u, team:t2, org:o2]`，
与 `t`／`o` 互不相干。团队 key 的用量是否也算到用户头上由宿主决定：链里带 `user:u`
就算。core 解析为这些 owner 的启用预算中 `model_pattern`（对整个
上游模型名的 `*`／`?` glob，空则全部）覆盖目标模型的那些。所有适用预算都必须有余量
（AND）：第一个 attempt 之前逐个读取当前窗口——没有就按 `(quota_id, starts_at_ms)`
懒开一行 `quota_windows`，带上额度行的快照——`used >= limit_value` 的窗口让请求以
`CoreError::BudgetExhausted { quota_id, window_key, resets_at_ms }` 失败，不碰任何凭证；
漏斗仍交付 `Failed` 的用量报告，开启 trace 时还有 `TraceEvent::BudgetRejected`。

周期有 `5h`、`1d`、`7d`（按 `anchor_at_ms` 对齐的固定窗口，未设则以 epoch 0 对齐；
其他周期名配 `period_seconds` 同理）、`1m`（UTC 自然月）和 `total`（唯一的永久窗口，
`ends_at_ms = None`）。过期窗口留作历史，下一次请求开下一个。
`Core::reset_budget(quota_id, now)` 在 `now` 关闭打开的窗口，把额度行的 `anchor_at_ms`
改为 `now`，再从 `now` 开一个新窗口（固定周期开整整一个周期，`1m` 到月末，`total`
永久）；新的 anchor 在重载快照发布后决定之后的窗口。`Core::budget_status(owners, now)`
向宿主与控制台报告 owner 的每个预算及其窗口、`used`、`limit` 和 `resets_at_ms`。

定价是 core 的事。结算时每次交换按快照的 `PriceBook`（`price_rules` 及其
`price_rates`、`price_tiers`）计价：Provider 规则先于全局规则，在 `model_pattern` glob
与可选操作都匹配的规则里 `(priority, id)` 最小者胜；档位按 `actual_service_tier` 与
`min_prompt_tokens` 选择；token 类按每百万计价，其他 metric 按费率行，条件对用量
dimensions 匹配（映射见 [pricing.rs](src/pricing.rs)）。结果写在 `ExchangeUsage::cost`
与 `UsageReport::cost` 上给 Observer。没有规则覆盖的交换计 0，并带
`dimensions["unpriced"] = "true"`。请求的费用随后按 `request_id` 幂等地
（`quota_settlements`）结算进每个适用预算的窗口——链上每个 owner 一条——只结算 unit
与费用币种相同的预算。
费用要交换结束才知道，所以预算最多被超出一个请求。

## 观测

Core 默认使用 `StoreObserver`，把上游日志写入 `capture_records` / `capture_events`，
每个请求的用量汇总写入 `usage_records`。`.observer(...)` 显式替换默认落库实现。

Setting 的 `enable_upstream_log`、`enable_upstream_log_body` 控制头信息与正文记录；
`enable_usage` 控制用量记录留存，`enable_settlement` 控制提取、计价与额度结算。
两个 usage/settlement 开关都关才停止提取；只关记录仍结算，只关结算仍可记录用量。
配置重载后对新请求生效，已有请求保持自己的快照。

每次真实上游调用独立落日志，包括重试与同一渠道调用内的多次 send。流 chunk 和 WS 帧
按序存入事件表；中断保留已收到的字节与部分用量。所有交换关闭并刷写后，请求只写一份
用量汇总。丢弃响应 body/socket、执行 future 或 `UsageCompletion` 都不会跳过收尾。
历史用户/key/订阅归属由宿主通过 `RequestContext.attribution` 提供，不从 scope 推断。
下游日志及 CaptureLink 由宿主负责。详见[观测契约](../../design/core-observation.md)。

## protocol 能力

| 类型 | trait | 绑定 |
|---|---|---|
| `AttemptUpstream` | `Upstream<Target = OperationKey>` | 一次 attempt 的 Provider、固定的凭证版本与 client；自持且可克隆，惰性启动的 fanout 子调用可超出 attempt 栈帧存活 |
| `ProtocolState` | `StateStore<Scope = StateScope>` | Store 的 ProtocolState 行；scope 是调用方 scope、Provider 与可选会话 |
| `Resources` | `ResourceAccess<Scope = ResourceScope>` | 经文件存储落 `resource_bindings` 与 `file_objects` 的发布；`Id` 先解析同 scope 的发布，再走 scope Provider 的 files API；`Url` 由匿名 client 在宿主 `FetchPolicy` 之下抓取（`resolve` 只做策略检查，`read` 才发 GET） |
| `ChannelStateStore` | `gproxy_channel::ChannelState` | 同一批 ProtocolState 行，限定到一个 Provider 与一个凭证，作为 `OperationContext.state` 交给渠道；渠道只选 key，看不到 scope |

按 URL 读取（请求里以链接给出的媒体：`image_url`、Gemini `file_uri`、Claude 的 URL
source）是 core 代调用方发起的服务端抓取，因此受 `FetchPolicy`
（`CoreBuilder::fetch_policy(Arc<dyn FetchPolicy>)`）约束。策略看到 URL 与解析器对其
主机返回的地址，回答 `Allow` 或 `Deny(reason)`；首个请求与每一跳重定向（最多三跳）都会
再问一次，公网链接无法把抓取弹到内网地址上。未设置时用 `DefaultFetchPolicy`：仅
http/https，拒绝 loopback、link-local、RFC 1918／`fc00::/7`、unspecified、broadcast 与
multicast 地址，含 IPv4-mapped 形式，无论地址来自 URL 的字面主机还是解析结果。单用户
宿主的网络就是调用方自己的网络，一行即可放开：
`.fetch_policy(Arc::new(AllowAllFetchPolicy))`；`AllowlistFetchPolicy::new(["cdn.example", "*.media.example"])`
只信任列出的主机名。抓取走 core 连接池缺省配置的普通 client，绝不走 scope 的凭证
client，Provider 鉴权不会流向任意 origin；body 受读取限额约束（先看 `Content-Length`），
`Content-Type`／`Content-Disposition` 填入返回的元数据。两处剩余风险留给宿主：出站
client 连接时会再解析一次域名，core 无法把连接钉在检查过的地址上（两次解析之间换绑的
域名会绕过检查——在意的话在前面放出口代理，或只 allow-list 自己掌控的域名）；wasm32
没有解析器，策略只能凭域名判断（字面 IP 主机仍会检查），且平台 `fetch` 自行跟随重定向。

URL 形态的发布（images 的 `response_format: url`）需要宿主提供链接构造器
`CoreBuilder::publication_url(Arc<dyn PublicationUrl>)`。core 没有公开 HTTP 面，所以
它只持有字节与绑定 id，链接由宿主签发：`url_for` 在任何写入之前用 core 即将记录的 id
调用，返回 `None`（或根本没配构造器）就以 `Unsupported` 拒绝，不留行也不留对象。宿主在
自己的路由上调用 `Core::read_publication(id)` 提供下载：对任何仍有效的发布返回元数据与
存储的字节，不检查 scope（路由已经认证了持链接者），过期、已释放或对象已从后端消失时
返回 `None`，其他后端故障以 `CoreError::File` 原样带出 opendal 错误；
`Core::delete_publication` 可提前墓碑化。images 族在第一次上游调用前就检查构造器，
没有构造器的宿主不会为无法交付的图片付费。

每次渠道调用还带 `OperationContext.instance_id`，即宿主进程的身份（`CoreBuilder::instance_id`，
缺省随机）。必须在两次请求之间保持上游活连接的渠道（claudeweb 在 `tool_use` 处停放
completion 流）把持有者记在续接记录里。之后的请求落到另一个进程时，attempt 以
`CoreError::ContinuationElsewhere { instance_id }` 失败：不重试、不给凭证记失败、续接原地
保留。多实例宿主要么把会话转发到那个实例，要么一开始就把会话粘住；单实例永远不会看到它。

## wasm32

依赖 SQLite／文件系统的集成测试只在原生平台运行，与 dev-dependencies 的平台限定一致。
wasm 检查包含库与可移植的测试目标，不表示已经在浏览器中执行测试。

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
