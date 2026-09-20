# gproxy-sdk

[English](README.md) | 简体中文

宿主嵌入用的 GPROXY 句柄。`gproxy-core` 只负责按调用方给定的 Provider 执行请求：
它不写配置、不解析模型名、也不知道别的实例改了什么。这一层补上其余部分——用默认
实现装配出 `Core`，承担推进 `settings.config_revision` 的配置写入，把一次登录变成
一行凭证，把模型名解析成执行计划，并通过共享 cache 与持久 revision 轮询让同一部署
的每个实例停在同一个 revision 上。

```rust
use gproxy_sdk::{GproxyBuilder, SyncMode};

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
```

`build()` 不会打开任何未被交给它的东西：同步实体 schema（除非关掉）、创建全局
settings 行、装配引擎、载入首个快照，并在 `SyncMode::Background` 下启动订阅与轮询
循环。密钥编解码器永不隐式选择：`master_key` 用 AES-256-GCM 密封，
`plaintext_secrets` 是显式的明文选项，两者都没有则直接拒绝构建。

## Features

| Feature | 作用 |
|---|---|
| `custom` / `codex` / `claudecode` / `claudeweb` | 编入对应渠道并默认注册 |
| `postgres` / `mysql` | 额外的 SeaORM 驱动，仅原生；原生始终自带 SQLite |
| `libsql` | 经 Hrana HTTP pipeline 的 libSQL／Turso，全平台 |
| `d1` | Cloudflare D1 绑定的标记，wasm32 本就自带 |
| `memory`（默认） | 进程内 `MemoryCache`，原生默认 cache |
| `redis` | Redis／Valkey，多实例部署用 |
| `fs`（默认） | 本地文件系统对象存储，仅原生 |
| `s3` | S3／R2 对象存储 |
| `bundled-vocabulary`（默认） | 内置 DeepSeek 词表用于 token 估算 |
| `ts` | 为 DTO 生成 `ts-rs` 声明 |

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
use gproxy_sdk::Gproxy;
use gproxy_core::{BudgetOwner, UsageAttribution};
use gproxy_protocol::{Dialect, Operation, OperationKey};

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
| `Transform`、`Route`、`Rewrite`、`OperationMismatch`、`InvalidTarget` | 否 |
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

## 不在这里的东西

- **身份**：用户、API key、组织、团队、权限、订阅、限流与 OAuth issuer 属于上层
  应用，它通过 `gproxy_store::load_all_data` 读同一个持久 revision。
- **下游鉴权**：这里不判断调用方是谁；句柄拿到的是 scope 以及允许的 Provider 与
  凭证集合。
- **server**：没有监听、路由、中间件或 CLI。原生与边缘宿主建在本 crate 之上。
- **执行**：尝试、协议转换、改写、观测、预算与结算都属于 `gproxy-core`，本 crate
  只决定交给它什么。
