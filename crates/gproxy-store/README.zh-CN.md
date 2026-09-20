# gproxy-store

[English](README.md) | 简体中文

GPROXY v4 的 SeaORM 2 实体、类型化批量仓储和原子持久化操作。原生 SeaORM 连接与
Cloudflare D1 使用相同 Store API；构造 Store 不打开数据库，也不自动修改 schema。

## Store 接口

[`Store<C>`](src/store.rs) 接收实现 `gproxy_seaorm::BatchConnectionTrait` 的连接。
所有实体入口共享 [`Repository<C, E>`](src/repository.rs)，普通 CRUD 不逐表手写。
全局设置是唯一单例入口（`id = 1`）。

```rust
use gproxy_store::{Store, entity::upstream::provider};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

let store = Store::new(connection);
let enabled = provider::Entity::find().filter(provider::Column::Enabled.eq(true));
let page = store.providers().page(enabled, 0, 50).await?;
let rows = store.providers().get_many(&["provider-id".to_owned()]).await?;
let control_data = store.load_control_data().await?;
```

| 方法 | 约定 |
|---|---|
| `create_many` | 调用方提供主键；批量插入及读取数据库默认值在同一 batch 内 |
| `get_many` | 保留输入顺序、重复 ID 和缺失项 `None`；支持复合主键 |
| `update_many` | 只修改显式 `Set` 的非主键字段；保留 `NotSet`／`Unchanged`；返回最终行或 `None` |
| `delete_many` | 按输入顺序返回影响行数 |
| `query`／`query_many` | 直接使用 SeaORM 条件、排序、JOIN，返回完整实体模型 |
| `update_where_many`／`delete_where_many` | SeaORM 条件批量修改，逐语句返回影响行数 |
| `count_many`／`page` | 分页前计数；总数和页面共享快照，并追加主键排序消除同值歧义 |
| `settings().get/update` | 读取或 upsert 第 1 行；只修改显式 `Set` 字段 |
| `load_control_data` | 一致的配置表快照，返回实体行，不解密 secret 或编译运行时对象 |

每个方法通过 gproxy-seaorm 执行一次原子 batch。`get_many` 按 D1 每语句 100 个参数的
上限拆分主键条件，仍在同一 batch 内；不自动拆分任意调用方 SQL，数据库请求／大小限制
仍然适用。空批次不做 I/O。`query` 必须选出模型解码所需字段；自定义投影和多实体结果
直接使用 gproxy-seaorm 的 batch API。一对多查询应先分页父表，再加载子项。

SQL 错误回滚事务写入；条件更新零行只返回冲突／行数，不回滚同批其他输入。
领域方法通过持久消费凭据或版本检查约束后续写入。不自动重试；提交结果不明确时先查
持久状态。CRUD 不运行 SeaORM ActiveModel hooks，也不负责宿主鉴权、全部业务字段校验、
加密、调度及网络操作；受保护的状态转换应使用领域方法，避免通过 CRUD 绕过约束。

[`operations`](src/operations/mod.rs) 仅包含原子领域操作：凭证刷新 CAS 与带版本检查的
状态变更、改写规则替换及
有序加载、额度幂等结算、带过期语义的协议状态 CAS、OAuth 签发／轮换／撤销／设备批准／
客户端退役，以及 agent 分配预留／启用／失败／当前目标读取。设备轮询及结果交付由签发层
负责，可用通用查询读取持久状态；订阅发放与价格计算仍由 core 完成。

## 首次初始化与增量 schema 同步

```rust
let store = Store::new(connection);
let report = store.sync().await?;
// 将 report.warnings 交给宿主日志，再初始化业务数据。
```

`Store::sync` 与 `schema(backend)` 共用一份实体注册清单。空库自动建表，已有库增量同步，
重复调用保留数据。由 `gproxy-seaorm::SchemaSyncConnectionTrait` 分别调用原生 SeaORM
schema sync 或 D1 的结构发现／规划／batch 执行。适配器已启用原生 `schema-sync`；
应用仍需选择 SeaORM 数据库驱动及运行时，D1 不链接原生驱动。

在启动阶段、开始接收请求前显式调用，由一个 schema 写入者执行。构造 Store 不做 I/O；
sync 不创建默认设置／管理员，也不自动运行版本化迁移。支持补充缺少的列和索引，改名／
索引调整遵循后端 sync 规则。类型修改、数据转换、已有外键变化等不支持的改动仍需显式
migration；不承诺原生所有后端的一次 sync 都是单个原子事务。
`SyncReport.warnings` 返回 D1 列类型差异；原生 SeaORM 的诊断通过日志输出。空列表不表示
所有结构变化都已完成；启动成功前应处理错误和诊断。

原有 `schema(backend).apply(&db)` 保留为一次性建表入口，已有表时会失败，不能每次启动
都调用。只有宿主显式调用这些 API 才会修改数据库，本次没有更新任何既有数据库。

## 精确金额与 schema 变更

`FixedDecimal` 固定 9 位小数，USD 最小单位为 `$0.000000001`，范围为
`-9223372036.854775808` 至 `9223372036.854775807`。JSON 使用十进制字符串。
解析及精确转换拒绝不能表示的精度和溢出；计算使用 `rust_decimal::Decimal`，最后对完整
结算金额显式调用 `FixedDecimal::rounded` 一次（中点取偶）。单价、数量及倍率共享此表示，
具体币种和单位仍由实体字段表达。

SQL 列为 BIGINT；实体通过 `save_as = "decimal(20,0)"` 和 `select_as = "char(32)"`
将整数原子值以文本跨越 D1 的 JavaScript 边界。数据库比较、排序和加法仍是数值运算。
手写 SQL 或 `col_expr` 写入 FixedDecimal 时必须调用该列的 `save_as` 转换。
结算操作在递增计数前检查非负金额和有符号整数溢出。

相较之前的 schema 草稿，decimal 列改为缩放整数；结算／设备批准新增尝试凭据，agent
会话新增待启用分配指针，数据库大小／刷新次数改用有符号 SQL 整数。本次没有迁移既有
数据库。已有 decimal 数据必须通过显式迁移转换；schema sync 不会转换旧值或修改列类型。

## 验证

```sh
cargo test -p gproxy-store -p gproxy-seaorm
cargo clippy -p gproxy-store -p gproxy-seaorm --all-targets -- -D warnings
cargo clippy -p gproxy-store -p gproxy-seaorm --target wasm32-unknown-unknown --all-targets -- -D warnings
```

永久测试覆盖原生 SQLite CRUD／约束、回滚、精确金额、状态过期、OAuth 消费与版本保护的
agent 切换。并发测试使用单条 SQLite 连接，不代表多服务器压力验证。实际 WASM Store
场景也通过了 Miniflare 本地 D1，详见[适配器验证记录](../gproxy-seaorm/VALIDATION.md)。
本次新 Store 操作没有执行真实 PostgreSQL／MySQL 或生产 Cloudflare 验证。

从 [`src/entity/mod.rs`](src/entity/mod.rs) 开始审阅，按业务分目录、一张表一个文件：

| 目录 | 实体 |
|---|---|
| `upstream` 上游 | Provider、Credential、Model、ProviderModel、OperationRule、OperationEndpoint、RewriteRuleSet、RewriteRule、ProviderRewriteRuleSet |
| `routing` 路由 | ExposedModel、Route、RouteMember |
| `identity` 身份 | Organization、Team、OrganizationMember、TeamMember、User、ApiKey、UserSession、Permission |
| `oauth` 下游授权 | Client、Grant、Code、Token、Device |
| `subscription` 订阅 | Pool、PoolMember、Plan、PlanLimit、Subscription |
| `limits` 额度 | RateLimit、Quota、QuotaWindow、QuotaSettlement、CredentialQuotaCycle、CredentialBlock |
| `pricing` 定价 | PriceRule、PriceRate、PriceTier |
| `usage` 用量 | UsageRecord、CaptureRecord、CaptureLink、CaptureEvent |
| `resource` 资源 | FileObject、AgentSession、AgentAssignment、ResourceBinding、ProtocolState |
| `config` 设置 | Setting、ConnectionProfile |

字段、主键／唯一键、关系和删除行为直接写在 SeaORM 属性中。
`schema(backend)` 把实体注册到 SeaORM schema builder。

结构覆盖上游与路由、用户与 API Key、权限与额度、定价、历史用量与调用、文件元数据、
资源映射、协议续接状态和设置。

代码中目前采用的待审设计：

- 业务 ID 使用调用方生成的字符串；时间戳使用 Unix 毫秒。全局设置使用 `id = 1` 的记录，
  明确列出网络、执行、执行限额（core 派生有限 limits 的超时与字节上限）、`config_revision`、
  词表、日志、存储选择、维护和用户门户字段。
- Provider 为全局配置。Credential 引用 Provider，同时归属组织、团队、用户三者之一；
  凭证归属与供应商配置分别建模。`status`（`active`／`dead`）加 `status_reason` 是持久
  生命周期，与运营的 `enabled` 开关分开：`set_status_many` 在版本 CAS 下记录确定性的刷新
  拒绝及原因，`refresh_many` 成功后回到 `active`。选择器只看这一位，原因是给人看的。
- 临时受限是 `CredentialBlock` 行：一行一个 block，含渠道 `QuotaScope` JSON、可选操作、
  `until_ms` 和 core 的 `BlockSource` JSON。core 的 cache 是热副本，Store 跨重启权威；
  过期行由 core 懒惰清理。删除凭证级联删除其 block。
- 独立 ConnectionProfile 保存后端（`reqwest`、`wreq`、`reqwest_native`）、代理模式／URL、wreq 模拟参数、解压开关、重定向、重试和连接池参数。
  三层可空 `connection_profile_id` 按凭证 → Provider → 渠道默认连接 → 全局 → 内置 reqwest／直连选择。
  `None` 继承整份配置；直连／系统代理是配置中的明确模式，不使用空 URL 表示。
  被引用的配置禁止删除（`ON DELETE RESTRICT`），没有 profile version。
  `gproxy-client` 按有效参数缓存 Client；已提供仓储 CRUD；宿主校验、继承和执行接线仍由 core 实现。
- Team 属于一个 Organization。OrganizationMember 和 TeamMember 分别保存 `member`／`admin`
  角色，同一用户在不同组织、团队中的角色可以不同。
- 组织凭证供组织成员及下属团队用户使用，团队凭证供团队成员使用；查看、编辑、删除这些
  共享凭证要求具备其归属组织／团队的管理员角色。当前 entity 只表达关系，尚未实现 API 权限判断。
- ProviderModel 可关联全局模型，删除模型资料时清空该可选引用。
- 路由结构为对外模型名 → Route → Provider／上游模型成员；不设置组织、团队、用户归属。
- Permission、RateLimit 暂留 user／API key 归属。Quota 可归用户、API Key、订阅或订阅池，
  四者恰选其一；归属一致性由后续写入层校验。
- 配置从属行使用代码中声明的删除行为。历史身份 ID 不建立配置外键；QuotaSettlement 关联
  QuotaWindow，与可以清理的 UsageRecord 独立。
- 文件内容留在 file/S3，文件实体只保存位置和元数据；模型自定义词表引用 FileObject。
- 金额／数量使用 9 位小数的 `FixedDecimal`，数据库保存有符号 BIGINT 原子值。
  D1 通过字符串和显式转换传输，不经过浮点数。
- 改写规则通过 RewriteRuleSet 复用，通过 ProviderRewriteRuleSet 按顺序绑定到供应商。
  RewriteRule 明确保存正则、替换文本、可选 JSON 点路径和筛选条件。删除规则集会级联
  删除规则及绑定；删除供应商只删除其绑定。已实现原子规则替换和有序加载；改写执行仍由 core 完成。
- 审计和派生用量汇总不在当前持久化层中。

改写规则新增 `target`（默认 `body`，另有 `header`／`query`）及可空 `target_name`。
Header／Query 选择已存在字段的值进行替换，保留重复项；Query 仅限 request。
`paths` 和事件筛选只属于 Body。目标／phase／名称合法性由规则编译阶段检查，Store CRUD
不执行改写。详见[改写设计](../../design/core-rewrite.md)。sync 新增 target 列时旧规则默认 Body。

## 方法级 URL

`operation_endpoints()` 提供 Provider 方法地址的通用批量 CRUD。每行保存 `provider_id`、
原生 `operation`、`dialect`、`transport`（`http`／`websocket`）、完整 `url` 和 `enabled`。
前四项构成唯一键，同一 Provider 的 OpenAI／Claude 生成方法可设不同 URL，同一 OperationKey
的 HTTP／WS 也可独立配置；流式生成是独立 operation，需要覆盖时单独设置。

地址配置由独立的 OperationEndpoint 保存，OperationRule 继续负责动作／操作映射。
新表已加入 sync 与 `load_control_data`；删除 Provider 会级联删除地址配置，不改变旧的
operation-rule 唯一约束。删除／停用一条地址即可回退默认值。

组装执行数据时校验 operation／dialect 及方法 URL 要求，将启用项加入
`ProviderData.operation_urls`。优先级为：方法 URL → Provider base_url＋渠道路径 → 渠道默认 URL。
方法 URL 是完整地址，不再追加默认路径；渠道定义的路径参数仍由对应方法处理。
动态返回的下载 URL，以及 OAuth／厂商 service 的辅助请求，不被这个设置统一覆盖。
当前完成持久化和 core 数据接口，调用接线仍未实现。

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
阈值和倍率须非负；条件须为非空对象且值为标量。这些是宿主写入层需要校验的契约，
当前 entity 未实现业务校验。缓存读取应从普通输入中扣除；推理／媒体 token 如果已包含
在总输入输出中，不能重复收费；按张与按 token 也不能无条件叠加。

已实现账本幂等结算；用量提取、条件选价和费用计算仍由 core 完成。新增指标不表示
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

已提供批量持久化，尚未接入日志采集、WS 轮次识别、聚合分摊或日志查询接口。

## OAuth

[`oauth`](src/entity/oauth/mod.rs) 表示 GProxy 向下游客户端签发授权，不是登录上游账号：

| 实体 | 记录内容 |
|---|---|
| Client | 公共 client_id、名称、回调地址、启用及软删除状态；不使用 client secret |
| Grant | 用户、内部 API Key、客户端、授权 scopes、ID token 身份字段、撤销及登录／刷新记录 |
| Code | 授权码 SHA-256 摘要、回调地址、PKCE S256 challenge、过期和一次性消费记录 |
| Token | access／refresh token 摘要、所属授权、过期、轮换消费凭据、撤销时间 |
| Device | 设备码摘要、用户短码、客户端、scopes、授权归属、批准／拒绝／消费时间，以及兼容 Codex 的密文结果 |

Client.id 就是公开的 OAuth client_id；Grant 关联 GProxy 用户及一个 `kind = oauth`
的内部 API Key，不绑定 Provider／Credential。内部 key 承载权限、用量归属，不能作为
普通 API Key 登录或导出。OAuth 身份仍走统一准入、路由、结算、日志路径，并取当前用户权限
与授权 scopes 的交集；不能获得控制台管理权限。UserSession 继续独立表示控制台会话。

授权码与 refresh token 的消费、令牌插入、登录／刷新统计必须在同一原子操作内提交。
`exchange_tokens_many` 使用每次尝试新生成的随机 `consumed_by` 消费凭据，保护后续令牌写入
和统计更新。`issue_many` 检查 key 用户／类型及设备与授权的客户端一致性；签发层校验用户同意、scopes 和回调策略。
`resolve_access_many` 检查 token 过期、撤销、Grant、Client、用户、内部 key 和订阅状态；
core 必须在每次访问及 WS 新轮次调用，并负责用户授权和 PKCE 策略。

撤销授权使用 revoked_at，客户端删除使用 deleted_at，保留会话历史；重新启用客户端
不得恢复旧授权。物理删除用户、内部 key 或客户端会级联清理授权及其码、token 和设备记录，
只用于最终清理。访问／刷新 token 不存明文；兼容 ID token 由签发层生成，签名密钥属于
宿主配置。state 由授权端点原样回传，PKCE verifier 不保存在普通授权码表。

上游 OAuth 仍属于 [`Credential`](src/entity/upstream/credential.rs)：密文 `secret` 的公共
内容结构为 `OAuthCredentialSecret`（access／refresh／ID token、类型、scopes、refresh
过期时间和供应商扩展字段）。`expires_at_ms` 表示 access token 到期；刷新需要按 version
条件更新密文及到期时间并递增版本，避免覆盖较新的凭证。加密封装及供应商刷新适配尚未实现。
上游登录过程的 state／PKCE verifier／device code 使用有过期时间、绑定发起人及 Provider
的缓存事务。交互式登录和凭证替换遵循归属管理权限；自动刷新由宿主执行，不改变普通
成员对共享凭证的使用权。上游登录／刷新均使用三级连接配置选择。

已实现原子签发、轮换、撤销、客户端退役及上游凭证 CAS。OAuth HTTP 端点、密钥管理、
上游网络登录／刷新和客户端集成仍由宿主完成。

### OAuth client ID 分层白名单

全局 `Setting`、`Organization`、`Team`、`User` 都有可空的 `oauth_client_allowlist`
字段，内容为 client ID 的 JSON 字符串数组。通过现有 settings update／repository 批量
CRUD 配置；客户端本身仍须登记在 `oauth_clients`，启用且未软删除。

- `None`／SQL NULL：本层未配置，不增加限制；全局未配置时仍受客户端注册表限制。
- `[]`：本作用域不允许任何 client；同级其他已配置作用域仍可通过并集允许它。
- 多个组织或多个团队：只对已配置的名单取并集；未配置的成员关系不会把并集放宽为全量。
- 全局、组织层、团队层、用户层之间取交集；下级不能放宽上级限制。某层没有任何已配置
  名单时该层不增加限制。用户与全局的空数组都表示完全禁止。
- 组织层包括用户直接加入的组织，以及所加入团队的所属组织；不要求额外存在组织成员行。
  管理员角色不绕过名单。ID 按字符串精确、区分大小写匹配，不支持通配符；非数组值和
  非字符串元素不会匹配。

例如全局 `[a,b]`、两个组织分别 `[a,c]`／`[b]`、团队 `[b,c]`，最终只允许 `b`；
用户再设置 `[a]`，最终为空，不会覆盖上层限制。

`oauth_clients().allowed_many(&[ClientAccess { user_id, client_id }])` 提供批量预检查。
`issue_many`（含设备批准）、授权码／refresh token 换取和 `resolve_access_many` 都在自己的
数据库语句中重新检查当前策略。策略拒绝返回冲突或空身份，且不消费源 token、不批准设备、
不创建依赖行。收紧后已有 token 在后续校验时失效；放宽后未过期且未撤销的 token 可以再次
通过，名单变更本身不等于永久撤销。core 应在每次请求及 WS 新轮次执行身份校验。

数据库 JSON 方言实现集中在 `gproxy-seaorm::json_array_contains_text`；Store 只组合
SeaORM 条件。D1 使用 SQLite JSON 扩展。当前开源 `sea-orm 2.0.3` 只有 SQLite、PostgreSQL、
MySQL 后端，MSSQL 属于单独的 SeaORM X，项目未接入；不支持的后端明确报错。
此次增加四个可空列，既有数据默认继承；未自动执行数据库 schema 更新。

## 订阅聚合与切分

[`subscription`](src/entity/subscription/mod.rs) 表达上游订阅池向下游发放 GProxy 虚拟订阅：

```text
Credential → PoolMember → Pool → Plan → Subscription → 多个 API Key / OAuth 会话
                               └─ PlanLimit → 复制为订阅的 Quota
Pool 的 Quota、订阅的 Quota → QuotaWindow → QuotaSettlement
```

- PoolMember 绑定一个上游凭证，以渠道提供的真实订阅 source_key 去重。一个真实订阅
  只计入一个池，切分发生在下游；更新 token 不创建新的容量来源。池成员不改变凭证归属。
- Pool 的 Quota 是可分配的预算；上游实际可用量来自成员的 CredentialQuotaCycle 观测。
  两者分开保存，不能把运营配置的预算当成上游报告的剩余量。
- Plan 定义 GProxy 套餐名称和 Codex／Claude Code 的展示字段。PlanLimit 定义每个订户的
  默认窗口、美元额度 `limit` 及模型范围；发放时复制到 subscription_id 归属的 Quota，
  固定使用 `metric = cost`、`unit = USD`，
  可在发放时调整额度。更改模板不追溯修改已有额度，已发放的套餐通过新 Plan 变更条款。
- Subscription 绑定用户和套餐，具有生效、到期、启用状态；API Key.subscription_id 指向
  所选订阅，OAuth 通过内部 key 共享同一关系。key 和订阅必须属于同一用户。暂停或到期
  后必须拒绝继续使用该订阅，不能退回无订阅模式；Plan.enabled 只控制新发放，Pool.enabled
  则控制该池是否继续承接请求。

额度聚合按同一指标、单位、模型范围及窗口类别分组。同一来源的五小时／七天限制是
同时生效的约束，不是两份可以相加的容量。不同账号保留各自重置时间，不为池捏造统一
的上游重置点。只有百分比而无对应基数时，绝对额度保持未知；过期或不完整观测不能
当作零用量。容量换算及观测有效性由后续聚合实现明确处理。

下游统一使用 USD 账本：输入输出、缓存、工具、图像等用量按价格规则转换为 USD 后扣款。
订阅计费要求价格规则以 USD 计价，不隐式混算其他币种。上游百分比、token 限制只描述
承接能力，不能直接相加成美元余额。池的可分配 USD 预算需要明确配置或另行估算。
例如池预算 $1000、每份订阅分配 $100；同窗口和范围的已分配金额应在发放／调整时校验。
窗口不一致时不能直接相加。下游固定周期
由订阅生效时间／Quota.anchor_at_ms 锚定，日周月使用 UTC，总量不重置；不随某个上游
账号重置而清空下游已用量。primary、secondary 等 window_key 供客户端适配器选择展示窗口。
下游展示自己分到的 USD 总额度、USD 已用／剩余及重置时间（协议要求百分比时由美元账本计算），不暴露各上游账号，也不直接展示整个池的
全部剩余额度。客户端套餐标签是 GProxy 分配视图，不改变真实上游订阅权益。

执行候选须同时满足路由目标、池成员、现有凭证使用权限。API Key 切换订阅只影响后续
请求／WS 新轮次；已开始的请求固定原订阅和池，历史 UsageRecord.subscription_id 与
上游 CaptureRecord.pool_id 保持不变。池预算按上游实际调用 ID 结算一次，用户预算按下游
请求 ID 结算一次；共享上游调用的下游费用分摊仍需显式结算策略。登录新的 OAuth 会话
不应自动复制出一份新额度。

复用现有配额窗口和幂等扣账表，持久化层没有第二套订阅用量账本。删除订阅会删除其 key、
OAuth 授权及配置配额，但历史窗口、结算、请求用量仍保留；通常通过停用／到期保留配置。
存在已发放订阅时不允许物理删除套餐。已有批量 CRUD 和幂等结算；尚未实现聚合计算、发放、
配额预占、候选筛选或 Codex／Claude Code 订阅接口渲染。

## 云端 agent 粘性与耗尽切换

[`AgentSession`](src/entity/resource/agent_session.rs) 是稳定的逻辑会话，按用户、服务／路由
scope 和下游 affinity_key 唯一标识；它与 CaptureRecord 的一次 WS 连接不是同一个概念。
固定策略为尽量使用同一份符合路由、模型、权限和订阅池要求的凭证，确认所需额度耗尽后
才因额度原因切换；普通瞬时限流不能直接认定订阅耗尽。切换成功后继续粘住新凭证，
旧凭证重置不会触发切回。会话的下游订阅及 USD 账本保持不变。

[`AgentAssignment`](src/entity/resource/agent_assignment.rs) 每代保存目标 Provider／Credential、
前一有效代、触发原因／额度观测或请求关联，以及准备、启用、替换、失败／结果未知状态。
不会覆盖旧目标。会话保存 version 和 active_generation，当前目标必须按
`(session_id, active_generation)` 查找，不能取最新一行。切换预留使用条件更新 version，
新代号采用本次预留版本且不复用；准备失败后的下一次尝试也使用新版本。
`pending_assignment_id` 将后续写入限定在本次预留的具体分配。

准备阶段在新凭证下按具体 API 建立或续接所需资源。只有准备完成，才能在同一原子提交中
验证会话版本、将新分配设为 active、旧分配设为 replaced，并更新 active_generation 和 version。
过期 worker／回调的旧版本不能覆盖当前分配；开始切换后新工作等待有效分配，旧调用保留
原代。上游创建结果未知时记录 uncertain，并利用资源查询或受支持的幂等键恢复结果，不能
仅因等待超时就重复创建。仓储已实现预留、启用和失败状态转换；远程准备及恢复仍由 core 完成。

[`ResourceBinding`](src/entity/resource/resource_binding.rs) 的唯一键增加 generation，
普通资源用 0，agent 资源对应分配代号。对外 server、environment、session 等 ID 可以稳定，
每代另存真实上游 ID、目标、依赖资源 ID、摘要和时间。远程 token 类映射的 public_id 存
宿主生成的下游 bearer 摘要，实际上游 token 放密文 secret，不混入公开摘要。
同一逻辑会话的资源 scope 绑定逻辑 session，不随上游切换变化；每个关联必须校验同一
用户／会话、匹配的代号及目标。多种句柄最终解析到同一 AgentSession 的当前有效代。

旧任务、文件、环境不会因为换凭证就自动迁移。普通新工作使用当前代，明确针对旧资源的
读取／管理继续使用其原目标；新目标下的复制／重建／续接由有对应能力的适配器完成，
没有能力时保留失败原因并阻塞该续接，不宣称透明无损迁移。续接检查点可使用现有
ProtocolState，scope/key 必须绑定用户、逻辑会话和分配代；状态字节应由宿主按需加密。

已开始的 WS／流式调用固定其 CaptureRecord.agent_assignment_id，不能中途换成另一账号的
数据流。切换只影响准备完成后的新调用／受支持的重新连接。日志中的分配 ID 是历史引用；
凭证／Provider 删除也不再级联清除 AgentAssignment 或 ResourceBinding 的目标记录。
目标配置不存在时必须返回不可用，不能把丢失绑定当作无约束选路。显式清理逻辑会话会
删除分配行、清空资源的 assignment_id，但保留资源代号／原目标，不将其变成普通第 0 代资源。

这部分依据 v3 的会话粘性、持久资源绑定及本地 Codex／Claude Code API 调查建模。
已实现并发切换的原子持久化，尚未实现耗尽判定、资源重建、token 映射
或真实 WS 续接；调查中的接口存在也不等于已证明支持跨账号迁移。
