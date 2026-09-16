# gproxy-store

[English](README.md) | 简体中文

GPROXY v4 的 SeaORM 2 entity 定义。**当前是实体审查稿**，尚未实现仓储读写，也未向
实际数据库应用迁移。

从 [`src/entity/mod.rs`](src/entity/mod.rs) 开始审阅，按业务分目录、一张表一个文件：

| 目录 | 实体 |
|---|---|
| `upstream` 上游 | Provider、Credential、Model、ProviderModel、OperationRule |
| `routing` 路由 | ExposedModel、Route、RouteMember |
| `identity` 身份 | Organization、Team、OrganizationMember、TeamMember、User、ApiKey、UserSession、Permission |
| `limits` 额度 | RateLimit、Quota、QuotaWindow、QuotaSettlement、CredentialQuotaCycle |
| `pricing` 定价 | PriceRule、PriceRate、PriceTier |
| `usage` 用量 | UsageRecord、CaptureRecord、CaptureLink、CaptureEvent |
| `resource` 资源 | FileObject、ResourceBinding、ProtocolState |
| `config` 设置 | Setting |

字段、主键／唯一键、关系和删除行为直接写在 SeaORM 属性中。
`schema(backend)` 把实体注册到 SeaORM schema builder。

草案覆盖上游与路由、用户与 API Key、权限与额度、定价、历史用量与调用、文件元数据、
资源映射、协议续接状态和设置。

代码中目前采用的待审设计：

- 业务 ID 使用调用方生成的字符串；时间戳使用 Unix 毫秒。全局设置使用 `id = 1` 的记录，
  明确列出网络、执行、词表、日志、存储选择、维护和用户门户字段。
- Provider 为全局配置。Credential 引用 Provider，同时归属组织、团队、用户三者之一；
  凭证归属与供应商配置分别建模。
- 代理使用独立的 `proxy` 列，优先级为 `Credential.proxy → Provider.proxy → Setting.proxy`，
  `None` 表示继承上一级；三级均未配置时，按全局 `inherit_system_proxy` 决定是否使用系统代理，
  否则直连。当前仅定义存储字段与继承契约，尚未接入网络执行层。
- Team 属于一个 Organization。OrganizationMember 和 TeamMember 分别保存 `member`／`admin`
  角色，同一用户在不同组织、团队中的角色可以不同。
- 组织凭证供组织成员及下属团队用户使用，团队凭证供团队成员使用；查看、编辑、删除这些
  共享凭证要求具备其归属组织／团队的管理员角色。当前 entity 只表达关系，尚未实现 API 权限判断。
- ProviderModel 可关联全局模型，删除模型资料时清空该可选引用。
- 路由结构为对外模型名 → Route → Provider／上游模型成员；不设置组织、团队、用户归属。
- Permission、RateLimit、Quota 暂留 user／API key 两种归属列，归属结构确认后再实现管理逻辑。
- 配置从属行使用代码中声明的删除行为。历史身份 ID 不建立配置外键；QuotaSettlement 关联
  QuotaWindow，与可以清理的 UsageRecord 独立。
- 文件内容留在 file/S3，文件实体只保存位置和元数据；模型自定义词表引用 FileObject。
- 金额／数量暂按 `Decimal(28,12)` 表达业务类型。当前 D1 适配器尚未实现该映射；实际接入
  D1 数据读写前，需要确认精度和存储表示。
- OAuth 签发记录、可复用修改规则集、审计和派生用量汇总不在首轮实体草案中。

## 路由结构

[`ExposedModel`](src/entity/routing/exposed_model.rs) 将全局唯一的对外模型名精确映射到
[`Route`](src/entity/routing/route.rs)，多个名称可以指向同一条路由。
Route 保存名称、启用状态、`round_robin`／`weighted`／`failover` 策略和正数
`max_attempts`（含首次调用，执行时受全局最大尝试次数限制）。

[`RouteMember`](src/entity/routing/route_member.rs) 保存 Provider、明确的上游模型名、
`tier`、`weight` 和启用状态。先选较低且可用的层级，同组内按轮询、权重或故障切换策略
选择成员；再由 Provider 的凭证策略选取可用凭证。模型名不建立模型目录外键。
当前 v3 的接口与执行路径不使用旧 schema 中的固定 credential_id 和 priority，本稿也不保留它们。

Namespace 从对外名称的首段派生：`coding/fast` 在全局以完整名称索引，在 `coding`
入口内以 `fast` 索引；没有独立 Namespace 表。命名入口沿用 namespace → 路由名 →
Provider 名的查找顺序。这里是模型映射，不是 alias 字符串改写。

删除路由级联删除其成员和对外映射；删除 Provider 只删除引用它的成员。路由配置没有
组织、团队、用户归属。非空名称、正数权重／尝试次数以及运行时选择规则是后续写入和
执行层的契约；当前只定义实体，不代表路由执行已经实现。

## 定价结构

- [`PriceRule`](src/entity/pricing/price_rule.rs) 按全局／Provider、上游模型和可选操作匹配；
  币种归属于规则。先匹配 Provider 规则，再回退全局，同范围按 `(priority, id)` 升序选择。
- [`PriceRate`](src/entity/pricing/price_rate.rs) 保存具体指标的价格、计量单位和数量分母。
  每行有独立 ID，同一指标可以有多个尺寸、质量、工具名等条件费率；按 `(priority, id)`
  选择首条全部条件匹配的费率，没有匹配则回退无条件费率，二者不叠加。
- [`PriceTier`](src/entity/pricing/price_tier.rs) 保存长上下文、服务层级及其组合的 token 价格。
  先选满足阈值的最高上下文档，再选实际服务层级的最高档；同阈值按 `(priority, id)` 升序。
  服务层级的显式分项价格优先，未设置的分项继承上下文价格并应用倍率；`None` 表示继承，
  `0` 表示免费。阈值包含输入、缓存读取和缓存写入，各计一次。

[`metric.rs`](src/entity/pricing/metric.rs) 明确列出内置计费指标，同时允许自定义指标：

| 类别 | 分项 | 计量方式 |
|---|---|---|
| 文本／Embedding | 输入、输出、缓存读取、缓存写入 5min／30min／1h、推理 | token，通常每百万 |
| 图像 | 输入／输出 token、生成图片 | token 或张；条件区分尺寸、质量 |
| 音频 | 输入／输出 token、缓存音频输入、时长、语音合成字符 | token、秒或字符 |
| 视频 | 输入／视频 token、时长、生成视频 | token、秒或个；条件可区分分辨率 |
| Rerank | 输入 token、搜索单元 | token 或次 |
| 工具 | 网页搜索、网页抓取、文件搜索、代码执行会话、其他工具调用 | 次或会话；条件可区分工具名 |
| 请求 | 按请求收费 | 次 |

费率金额使用规则币种，计算为 `数量 × value / unit_quantity`。分母须大于零，价格、
阈值和倍率须非负；条件须为非空对象且值为标量。这些是未来写入层需要校验的契约，
当前 entity 未实现业务校验。缓存读取应从普通输入中扣除；推理／媒体 token 如果已包含
在总输入输出中，不能重复收费；按张与按 token 也不能无条件叠加。

本次仅定义存储结构与指标名称，尚未实现用量提取、条件选价或结算；新增指标不表示
所有上游已能提供对应数量。

## 上下游交互与流式记录

`UpstreamCall` 已合入 [`CaptureRecord`](src/entity/usage/capture_record.rs)。一行保存
一侧的一次交互，包含调用摘要及完整的请求／响应结构；`side` 区分下游和上游。

| 实体 | 职责 |
|---|---|
| CaptureRecord | HTTP 交换、WS 连接或 WS 业务轮次；请求和响应放同一行 |
| CaptureLink | 下游交互 ID 与上游交互 ID 的多对多关联，以及该下游内的调用顺序 |
| CaptureEvent | 交互内按序记录的流式字节片段／WS 消息及方向、时间 |

HTTP 请求五要素为 `request_method`、`request_url`、`request_query`、`request_headers`、
正文；响应三要素为 `response_status`、`response_headers`、正文。查询字符串独立保存，
头部采用 `[名称, 值]` 对数组，保留重复参数和同名多值，与 WireRequest／WireResponse
的实际协议结构一致。

正文保存在数据库，不关联 FileObject。`Buffered` 使用 `request_body`／`response_body`；
流式则留空这些列，使用 CaptureEvent 按方向、序号拼接 payload，还原 SSE、NDJSON、
JSON 数组或字节流；保留分隔符，不把每个 chunk 当成一次调用。采集状态按请求和响应
分别记录，区分未采集、采集中、完整、部分和失败；交互另有完成／失败／取消状态，
不能仅凭 HTTP 200 判断流已成功完成。只开日志、不开正文采集时不必保存正文事件。
URL、查询、头部及正文沿用全局日志脱敏配置。

关联示例：

- 一对一：D1 → U1。
- 一对多／重试：D1 → U1、D1 → U2；每次真实上游调用有独立 ID。
- 多对一：D1 → U1、D2 → U1；U1 的正文、用量只保存一次。
- 多对多：直接组合这些边，不需要虚构一个共同 request_id。

边的 `sequence` 属于各自下游，不能作为上游记录的全局调用序号。删除一端只清理
对应关联边，不删除另一端。没有发生上游调用的下游错误也可独立记录。

WS 使用同一张表的两种记录：`WsConnection` 保存一次握手及连接生命周期，`WsTurn`
保存连接内每次业务交互，通过 `session_id` 关联同侧连接。握手才有 HTTP 方法、头部和
状态码，后续消息不伪装成 HTTP 请求／响应。全部 WS 消息事件挂在连接上（独立于握手正文），业务消息用可选 `turn_id` 关联轮次；
ping、pong、close 和无法归属轮次的消息不填写 turn_id。每条消息只存一份，
`(capture_id, sequence)` 主键保证连接内序号唯一，允许多个轮次交错。WS 记录应用消息，不记录 TCP 包或
底层 WS 分片。没有足够协议信息时保留连接级消息，不猜测轮次归属。

关联边连接 HTTP 交换／WS 轮次，因此可表达 HTTP ↔ WS，以及不同数量连接中业务轮次的
聚合／拆分；共享连接本身不意味着共享一次业务调用。写入层必须校验边的上下游方向、
轮次与连接的同侧关系和事件顺序。这些业务约束不是现有 entity 自动完成的校验。

`UsageRecord.request_id` 对应下游 HTTP 交换／WS 轮次 ID，与日志仅逻辑关联。
上游原始用量存于实际交互的 `metrics`，不能因多条边重复累加；共享调用如何向下游
分摊费用由结算层明确决定，关联本身不规定等分或全额重复扣费。

当前仍是实体草稿，尚未接入日志采集、WS 轮次识别、聚合分摊或日志查询接口。
