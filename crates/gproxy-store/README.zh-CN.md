# gproxy-store

[English](README.md) | 简体中文

GPROXY v4 的 SeaORM 2 entity 定义。**当前是实体审查稿**，尚未实现仓储读写，也未向
实际数据库应用迁移。

从 [`src/entity/mod.rs`](src/entity/mod.rs) 开始审阅，按业务分目录、一张表一个文件：

| 目录 | 实体 |
|---|---|
| `upstream` 上游 | Provider、Credential、Model、ProviderModel、OperationRule |
| `routing` 路由 | Namespace、Route、RouteTarget |
| `identity` 身份 | Organization、Team、OrganizationMember、TeamMember、User、ApiKey、UserSession、Permission |
| `limits` 额度 | RateLimit、Quota、QuotaWindow、QuotaSettlement、CredentialQuotaCycle |
| `pricing` 定价 | PriceRule、PriceRate、PriceTier |
| `usage` 用量 | UsageRecord、UpstreamCall、CaptureRecord |
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
- RouteTarget 引用 provider，并可覆盖上游模型名；路由不强制关联模型目录。
- Permission、RateLimit、Quota 暂留 user／API key 两种归属列，归属结构确认后再实现管理逻辑。
- 配置从属行使用代码中声明的删除行为。历史身份 ID 不建立配置外键；QuotaSettlement 关联
  QuotaWindow，与可以清理的 UsageRecord 独立。
- 文件内容留在 file/S3，文件实体只保存位置和元数据；模型自定义词表引用 FileObject。
- 金额／数量暂按 `Decimal(28,12)` 表达业务类型。当前 D1 适配器尚未实现该映射；实际接入
  D1 数据读写前，需要确认精度和存储表示。
- OAuth 签发记录、可复用修改规则集、审计和派生用量汇总不在首轮实体草案中。

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
