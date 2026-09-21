# gproxy-sdk

[English](README.md) | 简体中文

宿主嵌入用的 GPROXY 句柄。`gproxy-core` 只负责按调用方给定的 Provider 执行请求：
它不写配置、不解析模型名、也不知道别的实例改了什么。这一层补上其余部分——用默认
实现装配出 `Core`，承担推进 `settings.config_revision` 的配置写入，把一次登录变成
一行凭证，把模型名解析成执行计划，并通过共享 cache 与持久 revision 轮询让同一部署
的每个实例停在同一个 revision 上。这些决定背后的设计笔记见
[`design/sdk.md`](../../design/sdk.md)。

```rust
use gproxy_sdk::{GproxyBuilder, SyncMode};

# async fn example() -> Result<(), gproxy_sdk::SdkError> {
let gproxy = GproxyBuilder::sqlite("gproxy.db")
    .await?
    .master_key([0u8; 32])
    .sync_mode(SyncMode::Background)
    .build()
    .await?;

for channel in gproxy.channels() {
    println!("{} ({})", channel.display_name, channel.id);
}
println!("serving revision {}", gproxy.revision().0);
# Ok(())
# }
```

`build()` 不会打开任何未被交给它的东西：同步实体 schema（除非关掉）、创建全局
settings 行、装配引擎、载入首个快照，并在 `SyncMode::Background` 下启动订阅与轮询
循环。密钥编解码器永不隐式选择：`master_key` 用 AES-256-GCM 密封，
`plaintext_secrets` 是显式的明文选项，两者都没有则直接拒绝构建。

## Features

| Feature | 作用 |
|---|---|
| 某个渠道名（`codex`、`kiro`、`openai`……） | 编入该渠道并默认注册 |
| `channels` | 本工作区提供的全部渠道 |
| `postgres` / `mysql` | 额外的 SeaORM 驱动，仅原生；原生始终自带 SQLite |
| `libsql` | 经 Hrana HTTP pipeline 的 libSQL／Turso，全平台 |
| `d1` | Cloudflare D1 绑定的标记，wasm32 本就自带 |
| `memory`（默认） | 进程内 `MemoryCache`，原生默认 cache |
| `redis` | Redis／Valkey，多实例部署用 |
| `fs`（默认） | 本地文件系统对象存储，仅原生 |
| `s3` | S3／R2 对象存储 |
| `bundled-vocabulary`（默认） | 内置 DeepSeek 词表用于 token 估算 |
| `ts` | 为每个 DTO 与渠道描述符生成 `ts-rs` 声明，并带导出测试——见[类型导出](#类型导出) |

默认集可在 `wasm32-unknown-unknown` 上编译，`libsql` 亦然。wasm 上 cache 默认是
`gproxy_store::StoreCache`，同步一律手动：isolate 活不过一次请求，在请求开头调用
`tick()` 就是全部机制。

原生构建使用 `reqwest` 传输。想要别的后端（`wreq` 做 TLS 指纹、`reqwest-native`
对齐 Codex CLI 的栈）的宿主自行在 `gproxy-client` 上打开该 feature，或把自己的
`ClientPool` 交给 builder。

## 同步

两套机制，缺一不可。

| | 传递什么 | 会怎么失败 |
|---|---|---|
| 共享 cache 上的 `Invalidation` | 毫秒级的“再看一眼” | 丢消息：发布失败、订阅滞后、topic 关闭 |
| `settings.config_revision` 轮询 | 持久事实，默认 30s 一次 | 慢 |

通知从不携带状态：它只说存在哪个 revision，实例仅在该值比自己正在服务的更大时才
重载。无法解析的载荷、或指向本快照从未见过的凭证的通知，一律重载而不是猜测。
重载是串行且单调的——旧 revision 永远不会覆盖新的，重载失败则继续服务旧快照。

## 调用

`call` 组装请求，`send` 解析模型名并走完计划。scope 必填——它是 core 用来隔离各调用方
凭证亲和性的边界，没有安全的默认值。

```rust
use gproxy_sdk::{Gproxy, SdkError};
use gproxy_core::{BudgetOwner, UsageAttribution};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest};

# async fn example<C>(gproxy: &Gproxy<C>, request: WireRequest<HttpBody>) -> Result<(), SdkError>
# where C: gproxy_seaorm::BatchConnectionTrait + Send + Sync + 'static {
let execution = gproxy
    .call(
        OperationKey { operation: Operation::GenerateContent, dialect: Dialect::OpenAi },
        request,
    )
    .scope("user:u-1")
    .attribution(UsageAttribution { user_id: Some("u-1".into()), ..Default::default() })
    .budgets(vec![BudgetOwner::new("user", "u-1")])
    .credentials(["cred-1".to_owned()].into())
    .send()
    .await?;

let (response, usage) = execution.into_parts();
# let _ = (response, usage);
# Ok(())
# }
```

`connect` 是同一套 builder 的 websocket 握手版本。应用层决定的一切——允许的 Provider
与凭证、预算链、会话——都由调用方传入；这里不对任何人做鉴权。

## 解析

模型名按第一条命中的规则解析：

| 名字 | 解析为 | 尝试预算 |
|---|---|---|
| 没有模型 | 全部启用的 Provider，无上游模型 | `settings.max_attempts` |
| 暴露的模型名 | 该路由启用的成员 | 路由自己的 |
| `渠道/模型` | 该渠道的 Provider，优先目录里列了该模型的 | `settings.max_attempts` |
| `Provider 名/模型` | 那一个 Provider | `settings.max_attempts` |
| 其他 | `SdkError::UnknownModel` | — |

暴露名精确匹配，且先于前缀形式，所以运营者可以把字面量 `openai/gpt-5` 暴露出来。
前缀形式内部**渠道 id 优先于同名 Provider**：渠道 id 由构建固定、改不掉，而 Provider
名随时可以改。

候选随后按渠道、允许的 Provider id、允许的凭证 id 收窄——最后一项正是应用层把某个
组织的凭证挡在别人请求之外的手段。禁用、已退役、`Dead` 的凭证直接去掉。被临时封禁的
凭证在该 Provider 还有未封禁凭证时去掉；若某 Provider 的凭证**全部**被封禁，则保留并
排在所有健康 Provider 之后——限流是最后手段，不是故障。名字解析成功但无处可发是
`SdkError::NoTarget`，与"名字不认识"是两回事。

排序为 `(tier, 健康度, 权重倒序, 稳定 id)`。tier 是硬偏好；只有首段——与第一个候选
tier 与健康度相同的那一串——参与均衡，由路由策略决定：`RoundRobin` 按路由计数器轮转，
`Weighted` 把平滑加权选中的提到队首，`Failover` 原样保留。

## 失败转移

core 已经在单个 Provider 的凭证之间重试。`send` 是另一个维度，只在另一个 Provider
可能救得回来时才换下一个：

| 结果 | 换下一个 Provider |
|---|---|
| `NoUsableCredential`、`CredentialDead`、`RefreshContended` | 是 |
| `ContinuationElsewhere` | 是 |
| 任何 `Channel(..)` 错误，含传输失败 | 是 |
| 401／403／429／5xx 应答，或被拒绝的 websocket 升级 | 是 |
| `BudgetExhausted`、`Forbidden`、`Cancelled`、`DeadlineExceeded` | 否 |
| `Transform`、`Route`、`Rewrite`、`OperationMismatch`、`InvalidTarget`、`NotImplemented` | 否 |
| `Store`、`Cache`、`Secret`、`Limits`、`Assembly`、`File` | 否 |

尝试预算跨目标共享：每个目标最多拿到自己凭证数那么多次、且不超过剩余额度，因此一个
计划的上游调用次数不会超过它的 `max_attempts`。没有目标可换时，最后一个应答原样返回
——最后一个 Provider 的 429 就是调用方的 429，不会被换成别的错误。请求体缓冲一次以便
重放；超过 `max_request_body_bytes` 的流式体保持流式，计划裁剪为单个目标。

## 会话

`session::extract` 读出调用方的会话标识，它决定后续一段对话是否留在同一个凭证上。
标识只从真正发起请求的客户端读，绝不从即将转发到的上游猜。阶梯依次是：网关 header
`x-gproxy-session-id`（或显式 `session_id()`），然后 `thread-id`、`session-id`、
`x-claude-code-session-id`、`x-conversation-id`、`x-grok-session-id`，然后是入站形态
自己的 body 字段——Responses 的 `client_metadata.thread_id`／`session_id`、Claude 那个
**JSON 编码字符串** `metadata.user_id` 里面的 `session_id`、Gemini 的
`request.session_id` 与 `request.sessionId`——再是对话稳定前缀的 sha256 指纹，最后才是
请求级 id，并老实标成 `SessionSource::RequestFallback`。

不会在任意 JSON 里递归寻找叫 `session_id` 的字段；请求、轮次与缓存标识
（`x-grok-req-id`、`user_prompt_id`、`prompt_cache_key`、`previous_response_id`）都不是
会话。网关 header 在请求发往上游前被摘掉。完整规则、每个字段的调查依据与否定清单见
[`design/session-identity.md`](../../design/session-identity.md)。

## 凭证登录

`gproxy.login()` 把一个人的浏览器会话变成一行凭证。三种流程，取决于该 Provider 的渠道
实现了哪几种——`Gproxy::channels()` 会报出每个渠道的 `login_modes`，向渠道要一个它没有
的流程就是 `SdkError::Unsupported`。

| 流程 | 步骤 | 适用 |
|---|---|---|
| 授权码 | `authcode_start` → `authcode_complete` | 带 PKCE 的浏览器跳转 |
| 设备码 | `device_start` → 反复 `device_poll` | 在另一台设备上输入的验证码 |
| Cookie 交换 | `cookie_exchange` | 用户手上已有的会话 cookie |

凡是不属于上游的事都由 sdk 自己负责。**PKCE**：verifier 是这里生成的 32 字节随机数，
永不外发，只有它的 S256 摘要进授权 URL——没有 verifier，被截获的授权码毫无用处。
**CSRF state**：在这里生成、也在这里比对。`authcode_complete` 只收整条 `callbackUrl`
或一个裸 `code`，二选一；state 与会话开始时的不一致不仅被拒，还会顺手销毁该会话，重放
因此没有第二次机会。

待完成的会话存在共享 cache 的 `gproxy-sdk:v1:login:{id}` 下，带自己的 TTL，不在进程里。
这正是"A 实例开始的登录 B 实例能收尾"的前提——在负载均衡后面收尾的通常就是另一个实例
——也让被放弃的登录不留任何痕迹：key 自己过期，而过期的 key 就是 `SdkError::LoginExpired`，
与一个从未签发过的 id 无从区分。

**轮询节奏是调用方的事。** `device_poll` 只走一步就返回，这里既不 sleep 也不循环。
`Pending` 带着下次该等多久；上游的 `slow_down` 会改写这个间隔并对之后每次轮询生效。
`Denied` 与 `Expired` 都是终局，会话随之删除。

```rust
use gproxy_sdk::{Gproxy, SdkError, dto::{AuthCodeComplete, AuthCodeStart}};

# async fn example<C>(gproxy: &Gproxy<C>, callback_url: String) -> Result<(), SdkError>
# where C: gproxy_seaorm::BatchConnectionTrait + Send + Sync + 'static {
let started = gproxy
    .login()
    .authcode_start(AuthCodeStart { provider_id: "p-1".into(), ..Default::default() })
    .await?;
// 把人送到 `started.authorize_url`，等他回来之后：
let created = gproxy
    .login()
    .authcode_complete(AuthCodeComplete {
        login_session_id: started.login_session_id,
        callback_url: Some(callback_url),
        ..Default::default()
    })
    .await?;
println!("credential {}", created.credential_id);
# Ok(())
# }
```

登录成功就是一次普通的凭证插入，走的是管理写入的同一个原语：一个 revision、一次重载、
一次通知。密钥在语句构造**之前**就用配置好的 codec 密封，渠道与数据库之间没有任何一环
见过明文，也没有任何东西把它送回去——返回的只有凭证 id。两种 OAuth 流程的 `auth_kind`
是 `oauth`，cookie 交换是 `cookie`；归属三列原样透传；调用方没给标签时，默认标签由渠道
名与上游报出的账号拼成，并在该 Provider 的既有标签里去重。

## 管理面

`gproxy.manage()` 是写侧：每个配置家族一个入口，全部走同一个写原语。

| 家族 | 表 | CRUD 之外 |
|---|---|---|
| `providers()` | `providers` | `reset_routing_defaults(provider_id)` 一次提交清掉该 Provider 的操作规则与 URL |
| `credentials()` | `credentials` | `reveal_secret`、`set_status`、`refresh`、`quota_probe`、`quota_read`、`quota_reset`、`health_reset`、`limit_status` |
| `models()` / `provider_models()` | `models`、`provider_models` | |
| `routes()` / `route_members()` / `exposed_models()` | `routes`、`route_members`、`exposed_models` | |
| `connection_profiles()` | `connection_profiles` | |
| `settings()` | 唯一的 `settings` 行 | 只有 `get` / `update`；分实例组与日志组两组 |
| `rewrite()` | `rewrite_rule_sets`、`rewrite_rules`、`provider_rewrite_rule_sets` | `replace_rules(set_id, rules)` 整体替换一个规则集 |
| `endpoints()` | `operation_rules`、`operation_endpoints` | |
| `quotas()` | `quotas` | `budget_status`、`reset_budget`、`limit_status`、`reset_limit` |
| `pricing()` | `price_rules`、`price_rates`、`price_tiers` | |
| `transfer()` | 全部配置行 | `export`、`import` |
| `catalog()` | 不落库，除非被应用 | `channels`、`default_models`、`apply_default_prices`、`tls_presets`、`rule_presets`、`apply_rule_preset` |
| `connectivity()` | `provider_models` | `test`、`model_test`、`discover_models`、`apply_discovered` |
| `tokenizer()` | `file_objects`、`models.vocabulary_file_id`、`settings` | `vocabularies`、`fetch`、`progress`、`delete`、`auth`、`set_auth`、`reveal_auth` |

前十行是对各自表的普通 CRUD：`list(ListQuery) -> Page<Dto>`、`get(id)`、
`create(Write)`、`update(id, Patch)`、`delete(id)` 与 `batch(Vec<BatchItem>)`；
唯一的例外是 `settings()`，它只有一行、只能读与打补丁。后四个见
[运维目录](#运维目录)。id 给了就用、没给就
生成，时间戳一律 Unix 毫秒，小数以字符串传输，`credentials.secret` 永不出现在任何
DTO 里——只有 `hasSecret`，外加单独的 `reveal_secret`。

### 一次写入，一个 revision

```
commit_revision([语句…, 自增, 读回])   同一个事务
      ↓
reload   整体重载；仅动凭证状态的写入走 `reload_credentials`
      ↓
publish  Invalidation::ConfigurationChanged { revision, scopes }
```

行与 `config_revision` 自增在同一个事务里，因此落库的写入不会对同伴不可见，自增也
不会凭空发生。`batch` 无论涉及多少行都是这样一个事务；被拒绝的写入根本到不了数据
库——校验在前，revision 不动。

重载在通知之前，绝不在之后：不能让同伴收到一个本实例还服务不了的 revision。通知本
身是尽力而为：cache 拒绝发布只让部署多等一个轮询周期，记日志而不报错。

写入自己声明 `Scope`，这是调用方唯一的选择。`Scope::CredentialState` 走便宜的
`reload_credentials`，且**仅**适用于只改密钥、过期时间与生命周期状态的写入：凭证的
其余部分——标签、auth kind、metadata、连接配置、归属——在装配时就冻进了
`CredentialData`，必须整体重载。`Credentials::update` 会依据 patch 自行选择。

### 暴露模型名的保留前缀

暴露名按精确匹配，但带 `/` 的名字并不自由：解析时会把未知名字的第一段读作收窄前
缀。`codex/gpt-5` 意为“`codex` 渠道上的该模型”，`my-openai/gpt-5` 意为“`my-openai`
这个 Provider 上的该模型”。因此第一段是已注册渠道 id 或现有 Provider 名的暴露名永
远走不到自己的路由，写入时即被拒绝，而不是留到运行期静默失效。其余都没问题：
`coding/fast` 就是个好名字。

## 运维目录

四个不是“单表 CRUD”的家族：搬运整份配置、这个二进制自带的数据、两个会走出进程的探
针，以及本地 token 估算读的词表文件。

### 导出与导入

`manage().transfer()` 把实例写成一份文档，也能把一份文档读回来。文档就是管理面自己
的 DTO，所以导出的内容正是控制台会列出来的内容。

```json
{
  "formatVersion": 4,
  "exportedAtMs": 1758412800000,
  "secretsOmitted": false,
  "secrets": ["aes-gcm"],
  "data": {
    "connectionProfiles": [], "providers": [], "credentials": [],
    "models": [], "providerModels": [],
    "routes": [], "routeMembers": [], "exposedModels": [],
    "operationRules": [], "operationEndpoints": [],
    "rewriteRuleSets": [], "rewriteRules": [], "providerRewriteRuleSets": [],
    "quotas": [], "priceRules": [], "priceRates": [], "priceTiers": [],
    "settings": null
  }
}
```

`data` 按回放顺序排列：任何一行都不会出现在它所指向的那一行之前。凭证是它自己的列
加上 `secret`，后者是 `{ "codec": "aes-gcm" | "plaintext", "bytes": "<base64>" }`
——原样搬运的数据库列，从不打开，也从不是明文。`include_secrets: false` 时每个
`secret` 都是 `null`，`secretsOmitted` 为真，`secrets` 为空。

**不会搬运的东西。** 身份（用户、API key、组织、团队、权限、订阅、OAuth client）属
于应用层，本 crate 根本够不着。用量记录、配额窗口、计数窗口、结算、凭证周期与封
锁、抓包、agent 会话、协议状态、cache 行与文件对象都是观测与运行期状态：搬过去等于
给目的地伪造一段它从未有过的历史。因此 `models` 的 `vocabularyFileId` 或 `settings`
的 `defaultVocabularyFileId` 指向目的地没有的文件时会被清空并给出 warning，而不是拒
绝整份文档——词表在那边重新拉一次就是了。

`import` 整份文档是一次 revision 提交，所以任何一处被拒绝都不会留下半截状态。
`Merge` 按 id upsert，文档没提到的一概不动；`Replace` 额外删掉文档省略的那些导出
表的行，先子后父，且绝不碰身份、用量与抓包表。文档内外都解析不到的引用会在写任何
东西之前被点名拒绝。

**主密钥规则。** 密封的 blob 只在封它的那把钥匙下打开，而 core 在装配快照时会打开
每一条凭证的密钥——所以一条打不开的凭证被导入，坏掉的不是它自己的调用，而是整个实
例之后的每一次重载。于是：

| 导入方持有 | 凭证的结局 |
|---|---|
| `sourceMasterKey`（源实例的 32 字节，标准 base64） | 打开一次并用本实例的 codec 重新密封，计入 `credentialsResealed` |
| 与源相同的 codec，没有密钥 | 原样写入，并给出 warning |
| 两者都没有 | 连同其行一起跳过，计入 `credentialsSkipped` 并给出 warning |

因此一份只含配置的导出不会新建任何凭证；它会更新目的地已有的那些，其余计入 skipped。
`ImportReportDto` 带有 `created`、`updated`、`skipped`、`credentialsResealed`、
`credentialsSkipped` 与 `warnings`——被静默清掉的归属或词表引用就报在 warnings 里。

### 静态目录

`manage().catalog()` 的答案来自这个二进制，不来自数据库。

- `channels()`——每个编译进来的渠道作为 `ChannelDescriptor`：登录方式、能力，以及
  Provider 表单该渲染哪些配置键。与 `Gproxy::channels()` 是同一份列表。
- `default_models()`——自带的模型目录：这个版本知道的模型名、上下文窗口与默认价
  格。它是生成资产时的快照，不是实时目录。
- `apply_default_prices({ providerId, modelIds, overwrite })`——把目录价格写成
  `price_rules` 及其 rates 与 tiers，一次提交。不给 Provider 时用目录自己的
  `*fragment*` 通配与 priority，一条规则就给该模型在所有 Provider 上定价；给了
  Provider 则用字面模型名作 pattern、priority 为 0。`overwrite: false` 正是重复应用
  目录也安全的原因——运维改过的规则保留其修改，并报为 `skipped`。
- `tls_presets()`——六种客户端身份，形式就是 `gproxy_client::EmulationConfig` 对
  象，可直接存进连接配置的 `emulation`。只有 `wreq` 后端会呈现它。
- `rule_presets()` / `apply_rule_preset({ ruleSetId, presetId })`——让某个客户端应用
  看起来像个通用客户端的改写规则集。应用预设是**替换**规则集而不是合并：一个预设是
  一份有序的完整答案，掐一半跟别的交织在一起，改出来的文本没人预料得到。想保留既有
  规则的调用方读 `rule_presets()`，然后把自己的列表交给 `rewrite().replace_rules`。

### 两个探针

`manage().connectivity()` 是本 crate 唯一走出进程的部分。

`test({ scope })` 通过 scope 指定的客户端链去问 Cloudflare 的 trace 端点，这个部署
从外面看是什么样——`Global`（实例默认配置）、`Provider`、`Credential`（就是它的调用
真正用的那条传输），或者 `Proxy { url }`，用来测一个还没配置在任何地方的代理。返回
`{ ok, latencyMs, ip, colo, error }`。**网络失败是 `ok: false` 加一个原因，不是
`Err`**：“上游不可达”正是提问者要的答案。`Err` 意味着请求本身错了——Provider 不存
在，或凭证不属于这个 Provider。

`model_test({ providerId, model, credentialId })` 与
`discover_models(provider_id, credential_id)` 完全按调用方请求的路径走 `Core`。
**它们会花掉一条真实凭证、消耗真实的上游配额、对该凭证所受的预算结算，并写一条用量
行**，归属到 scope `gproxy-sdk:admin` 与 API key id `admin-probe`。没有 dry run：没
真打上游的测试什么也没测到。一次尝试、三十秒 deadline、自身不带预算。

`discover_models` 用 Provider 自己的 dialect 去问，因此不发生任何转换，拿到的就是上
游自己的名字；每一个都带 `known`（该 Provider 已有 `provider_models` 行）与
`hasDefaultPrice`（自带目录能给它定价）。`apply_discovered(provider_id, names)` 插入
这些行并跳过已有的，所以同一次发现应用两遍与一遍等价。

### 词表

core 会为上游没报用量的交换做本地 token 估算，依据就是词表文件。
`manage().tokenizer()` 负责取它们。

`fetch({ repo, filename, modelId, setAsDefault })` 下载
`https://huggingface.co/{repo}/resolve/main/{filename}`（默认 `tokenizer.json`），把
存好的来源 token 作为 `Authorization: Bearer` 发出，经配置好的文件存储写入字节，然
后插入 `file_objects` 行并把 `models.vocabulary_file_id` 与
`settings.default_vocabulary_file_id` 指过去——行与两个指针在同一次 revision 提交
里。字节先落地：指向一个从未写入的对象的行会让之后每次重载都失败，而没有行的对象只
是浪费空间。非 2xx 会带着上游自己的状态码被拒绝，因为对刚敲完仓库名的人来说 `404`
和 `401` 完全是两回事；大小上限取实例的 `maxResponseBodyBytes`。没有配置文件存储
时，整个家族答 `Unsupported`：字节无处可放。

`progress()` 报告**本进程内**正在跑的下载——一个单元，不按句柄区分，也不跨实例共
享，所以控制台在一个请求里发起 fetch、在另一个请求里轮询进度。`delete(file_id)` 在
一次提交里删掉行并顺带释放所有指向它的选择，然后移除已存的对象。

`auth()` 只说是否配置了来源 token；`set_auth(token)` 把它像凭证密钥一样密封进
settings 行，绑定在一个固定身份上，因此数据库的副本里没有可用的 token；
`reveal_auth()` 是唯一一次有意的披露，单独成一个调用的理由与“读出凭证明文”不和“列
出凭证”混在一起是同一个。

## 查询

`gproxy.query()` 是同一批行的读侧：三个家族，读引擎留下来的东西。这里没有任何写入，
因此也不动 revision。

| 家族 | 回答 |
|---|---|
| `usage()` | `records(UsageRecordQuery)`、`summary(UsageQuery)`、`group(UsageGroupQuery)`、`trend(UsageTrendQuery)` |
| `quota()` | `windows`、`settlements(window_id)`、`credential_cycles(credential_id)`、`counted_windows(credential_id, now)`、`budget_status(owners, now)` |
| `logs()` | `list(LogQuery)`、`detail(request_id)` |

用量记录与配额窗口用管理面那套 offset 分页 `Page<T>`，请求日志用游标。这不是风格选
择：管理列表是人翻的有界集合，请求日志则是一边被读一边在增长的追加流，offset 分页
在它上面会重复或漏掉行。

### 聚合在 Rust 侧完成，且有扫描上限

`usage_records.metrics` 是每个请求一份 JSON 文档——归一化的 token 计数、它们背后的
按 exchange 明细、结算状态、定价成本。本 crate 支持的任何后端都无法在其内部求和，
因此 `summary`、`group`、`trend` 读出匹配的行在这里折叠，按键序分块读取而不是一次
全部载入。

这是有界的。每个聚合都接受 `maxScanRows`，缺省即 `query::MAX_SCAN_ROWS`（50 000）
并被它钳住；用满预算的聚合会带着 `truncated: true` 与 `scanned` 计数返回，而不是把
一个更小的数字当成全部事实。`trend` 还有第二重边界：`bucketMs` 为零或负、区间反向、
以及会产生超过 `query::MAX_TREND_BUCKETS`（5 000）个桶的区间，一律直接拒绝。

有两个数不来自文档。成本读自带索引的 `cost` 列——结算时写一次；只有按 Provider 的
切分会去读 `metrics` 里的定价金额，因为明细只存在于那里。而当被扫描的记录对币种不
一致时 `currency` 为 `None`：把美元和欧元加进同一个数字不叫合计。

因此按 `provider` 分组是按 exchange 而不是按行的：一个从某 Provider 失败转移到另一
个的请求会同时计入两者，各自带着那次尝试自己的 token 与价格，`requests` 对每个不同
的 Provider 只计一次。完全没有到达上游的记录落在空键下，这样各组仍然能加回总计。

单条记录上的 token 计数是 `Option<u64>`——上游没报告某个字段不等于它测得零——而所有
合计都是普通 `u64`，因为“什么都没报告”的和确实是零。

### 日志游标

`logs().list` 返回 `nextCursor` 与 `nextCursorId`，原样传回去就是下一页。两半都需
要：游标按 `(started_at_ms, id)` 降序，而两个请求可能起始于同一毫秒，只有时间戳的
游标要么永远在这一对上打转，要么直接跳过它。列表到头时 `nextCursor` 为 `null`，这
是事实而非猜测——查询多读一行，而不是从“这一页是满的”去推断。

只列出 `side = downstream` 的行。上游尝试不是一个请求，而是某个请求做过的事，由
`detail(request_id)` 通过 `capture_links` 解析出来——不走来源列，因此重试过的请求能
看到每一次尝试，被两个下游请求共享的上游调用也不会被复制成两条。

响应体上限 `query::MAX_BODY_BYTES`（64 KiB），事件上限
`query::MAX_DETAIL_EVENTS`（2 000），两处截断都会如实汇报。每个 body 都以
`LogBodyDto` 传输并带上该行自己的 `body_state`，因为从未被捕获的 body 不能读起来像
一个空 body——`notCaptured` 且零字节与 `complete` 且零字节是两件不同的事。文本仍是
文本，其余一律 base64。

### 脱敏发生在写入时

`logs()` 不做任何脱敏。这些行是 core 的 observer 按部署的日志脱敏策略写下的——
header、查询参数、流里的密钥片段——所以存下来的已经就是可以展示的。宿主不能假定读取
时还有第二遍：密钥若在库里，是因为策略允许它在那儿，读时过滤也挽回不了。

### 读侧不重复的东西

实时状态留在计算它的地方。`manage().quotas()` 拥有 `budget_status`、`reset_budget`、
`limit_status` 与 `reset_limit`；`query().quota().budget_status` 就是同一个 core 调
用、只是把时钟作为参数传入，而 `counted_windows` 读的是 core 没有对应状态的那些维度
的原始计量行——对 `limit:{quota_id}` 维度，权威答案是 `limit_status`，它知道背后的
quota 行，并以归一化小数而不是成本计量器所用的定点原子单位报告用量。

## 类型导出

`ts` feature 给 `dto` 导出的每个类型生成一份 `ts-rs` 声明，由一个测试写出去：

```sh
GPROXY_TS_OUT=console/src/generated \
  cargo test -p gproxy-sdk --features ts export_types
```

没有 `GPROXY_TS_OUT` 时测试直接返回、不写任何文件，因此
`cargo test --all-features` 不产生副作用，生成目录只会在有意为之时被重写。给了变量
则先清空目录——一个已经不存在的 DTO 留下的旧声明，会在 Rust 侧删掉它之后很久仍然在
控制台里通过类型检查——最后写一份把全部类型再导出一遍的 `index.ts`。

渠道目录也在其中：`dto` 把 `gproxy-channel` 的 `ChannelDescriptor` 及其组成类型再
导出一遍，本 crate 的 `ts` feature 会连带打开那个 crate 的。控制台渲染 Provider 表单
用的就是渠道自己返回的描述符，那么类型也应该由这个描述符生成，而不是一份下次渠道加
配置键就会失真的手抄件。

声明说了什么：

| Rust | TypeScript | 为什么 |
|---|---|---|
| `i64` / `u64` | `number` | `with_large_int("number")`。这里每个时间戳与字节数都远在 JavaScript number 能精确表示的范围内 |
| 小数金额 | `string` | 出于同一个理由，金额与限额在线上本来就是十进制字符串 |
| `serde_json::Value` | `unknown`，或该列被校验成的形状 | `paths` 确实是 `string[]`，`corsOrigins` 确实是 `string[]`；`config` 与 `metadata` 由渠道定义，保持 `unknown` |
| 带 tag 的枚举 | 同样带 tag 的联合 | `ts(tag = …)` 逐字段镜像 `serde(tag = …)`，联合就是线上形状 |
| `Option<T>` | `T \| null` | |

动手写之前值得知道两件事。可空列的 patch 字段是 `Option<Option<T>>`，生成
`T | null | null`，TypeScript 读作 `T | null`；第三种状态——键缺席，意为"这一列别动"
——没法在生成的声明里表达，所以调用方用 `Partial<CredentialPatch>` 构造 patch，那正
是对的形状。另外两个扁平化了 JSON 对象的目录类型（`DefaultModelDto`、
`DefaultModelCatalogSourceDto`）会生成为与一个 `JsonValue` 索引签名的交叉类型，
`JsonValue` 落在 `serde_json/` 子目录里。

`src/dto/export.rs` 里的清单是手写的，因为 Rust 没有运行期枚举模块类型的办法。v3 漂移
的正是这份清单——加了 DTO，没人记得改清单，控制台就悄悄少了一个类型——所以第二个测试
会读 `src/dto/mod.rs` 的源码，把它 `pub use` 的类型名与清单对比。加了 DTO 却忘了清单，
是一个红色的测试，不是一个缺失的文件。

## 不在这里的东西

- **身份**：用户、API key、组织、团队、权限、订阅、限流与 OAuth issuer 属于上层
  应用，它通过 `gproxy_store::load_all_data` 读同一个持久 revision。
- **下游鉴权**：这里不判断调用方是谁；句柄拿到的是 scope 以及允许的 Provider 与
  凭证集合。
- **server**：没有监听、路由、中间件或 CLI。原生与边缘宿主建在本 crate 之上。
- **控制台**：本 crate 只生成界面所依据的 TypeScript 类型；界面本身、它的会话处理与
  HTTP 传输都属于应用层。
- **执行**：尝试、协议转换、改写、观测、预算与结算都属于 `gproxy-core`，本 crate
  只决定交给它什么。
