---
title: "嵌入核心库"
description: "crate 依赖图、两级执行入口、宿主 trait 及其保证、边界类型、wasm 说明、作为参考嵌入方的 gproxy-app，以及可独立使用的库"
---

`gproxy-core` 是一个库。`gproxy` 二进制是它的一个宿主，Edge bundle 是另一
个，你的应用可以是第三个。工作区中没有任何 crate 发布到 crates.io；嵌入意
味着对本仓库的 git 或 path 依赖：

```toml
[dependencies]
gproxy-core = { git = "https://github.com/LeenHawk/gproxy", branch = "3.0" }
gproxy-channels = { git = "https://github.com/LeenHawk/gproxy", branch = "3.0" }
```

工作区为 Rust 2024 edition，版本 `3.0.0`。公开接口尚未按 semver
稳定。

## Crate 依赖图

| Crate | 一句话说明 |
| --- | --- |
| `gproxy-protocol` | 操作分类、`OperationSpec` 注册表，以及 OpenAI、Claude、Gemini 和 AWS 的线路模型。只依赖 `serde` 和 `http`；可编译到 wasm。 |
| `gproxy-transform` | 协议族之间纯粹的成对请求、响应和流转换。 |
| `gproxy-channel-api` | 通道契约：`Channel`、服务面、`NormalizedUsage`、`BindingStore`，以及线路原语 `ByteStream`、`WsDuplex`、`BoxFuture`。 |
| `gproxy-channels` | 内置 Provider 适配器（28 个通道；`claudeweb` 仅原生）。 |
| `gproxy-core` | 引擎：`Core`、宿主 trait、边界类型、控制面读模型、结算漏斗、规则。嵌入方唯一必须依赖的 crate。 |
| `gproxy-upstream` | 规范的 `UpstreamTransport`：原生上是 `WreqTransport`（TLS 配置、代理、WebSocket），wasm 上是 `FetchTransport`。 |
| `gproxy-tokenize` | 离线 token 统计：tiktoken、Hugging Face 词表、字符估算。 |
| `gproxy-store` | schema 目录、迁移、四种 SQL 后端和缓存后端。 |
| `gproxy-admin` | 基于 `http` 类型、不依赖框架的 admin 与门户分发；DTO 通过 `ts-rs` 导出为 TypeScript。 |
| `gproxy-app` | 参考嵌入方：配置、引导、ArcSwap 快照控制面、宿主服务、v2 迁移。 |
| `gproxy-host-axum` | 原生监听器和 `gproxy` 二进制：axum、静态资源、登录启动、自更新、公告。 |
| `gproxy-host-edge` | 面向 Cloudflare、Deno 和 Netlify 的 fetch 式 wasm 宿主。 |

依赖只有一个方向。宿主依赖 `gproxy-app`；`gproxy-app` 依赖 `gproxy-core`、
`gproxy-store` 和 `gproxy-admin`；`gproxy-core` 依赖 `gproxy-channel-api`、
`gproxy-channels`、`gproxy-transform` 和 `gproxy-protocol`。核心库从不依赖
服务器框架、数据库或 UI。

## 两级执行入口

来自 `crates/gproxy-core/src/api.rs`：

```rust
pub struct Core<H: Host> { /* private */ }

impl<H: Host> Core<H> {
    pub fn new(host: H, channels: ChannelRegistry) -> Result<Self, InitError>

    pub async fn invoke(
        &self,
        control: &dyn ControlPlane,
        target: &Target,
        ctx: RequestCtx,
    ) -> Result<ExecOutcome, CoreError>

    pub async fn execute(
        &self,
        control: &dyn ControlPlane,
        ctx: RequestCtx,
    ) -> Result<ExecOutcome, CoreError>

    pub async fn execute_planned(
        &self,
        control: &dyn ControlPlane,
        ctx: RequestCtx,
        plan: Plan,
    ) -> Result<ExecOutcome, CoreError>

    pub fn matches_ingress(&self, method: &http::Method, path: &str, upgrade: bool) -> bool
}
```

- **第一级 `invoke`。** 一个请求，以目标通道原生支持的线路形态，走一个选定
  的凭证。没有路由、转换和故障转移；过期时的 token 刷新和完整的结算漏斗仍
  然生效。服务面转发正是用它。
- **第二级 `execute`。** 完整引擎：分类、认证、别名与变体预处理、从
  `ControlPlane` 解析 `Plan`、准入、转换、通道、传输、故障转移、漏斗。
- **自带计划的第二级 `execute_planned`。** 跳过解析，自行提供有序目标和预算。
  分类、转换、预算内的故障转移和结算仍然运行。

当已注册的通道需要宿主未提供的能力时，`Core::new` 在启动时而非流量中拒绝：

```rust
pub enum InitError {
    SurfacesWithoutBindings { channel: &'static str },
    ResourceAffinityWithoutBindings { channel: &'static str },
    ContinuationsUnavailable { channel: &'static str },
    ContinuationSpawnerUnavailable { channel: &'static str },
    SessionMeterUnavailable { channel: &'static str },
}
```

## 宿主 Trait

宿主契约在 `crates/gproxy-core/src/host.rs`；`BindingStore` 位于
`crates/gproxy-channel-api/src/surface/state.rs`，`ControlPlane` 位于
`crates/gproxy-core/src/control.rs`。每个异步方法都返回工作区的
`BoxFuture`：原生上是 `Pin<Box<dyn Future<Output = T> + Send + 'a>>`，wasm
上没有 `Send`。`MaybeSend` 和 `MaybeSync` 在原生上即 `Send` 和 `Sync`，在
wasm 上为空。带默认实现的方法以 `{ ... }` 表示。

```rust
pub trait Host: MaybeSend + MaybeSync + 'static {
    type Credentials: CredentialStore;
    type Cache: CacheBackend;
    type Transport: UpstreamTransport;
    type Usage: UsageSink;
    type Capture: CaptureSink;

    fn credentials(&self) -> &Self::Credentials;
    fn cache(&self) -> &Self::Cache;
    fn transport(&self) -> &Self::Transport;
    fn usage(&self) -> &Self::Usage;
    fn capture(&self) -> &Self::Capture;
    fn authenticate<'a>(
        &'a self,
        request: &'a RequestCtx,
    ) -> BoxFuture<'a, Result<CallerIdentity, CoreError>>;
    fn admit<'a>(
        &'a self,
        identity: &'a CallerIdentity,
        request: &'a RequestCtx,
        operation: Option<OperationKey>,
        plan: &'a Plan,
    ) -> BoxFuture<'a, Result<(), CoreError>>;
    fn finish_admission<'a>(
        &'a self,
        request_id: &'a str,
        settlement: Option<&'a Settlement>,
    ) -> BoxFuture<'a, ()>;
    fn admit_credential<'a>(
        &'a self,
        target: &'a crate::control::Target,
        body: &'a bytes::Bytes,
    ) -> BoxFuture<'a, Result<(), CoreError>>;
    fn count_tokens<'a>(
        &'a self,
        model: &'a str,
        body: &'a bytes::Bytes,
        tokenizer_map: Option<&'a serde_json::Value>,
    ) -> BoxFuture<'a, Result<u64, CoreError>>;
    fn record_credential_health<'a>(
        &'a self,
        credential: CredentialId,
        model: &'a str,
        credential_version: u64,
        health: CredentialHealth,
        response_status: Option<http::StatusCode>,
        detail: &'a str,
    ) -> BoxFuture<'a, ()>;
    fn observe_credential_quota<'a>(
        &'a self,
        credential: CredentialId,
        observations: Vec<gproxy_channel_api::QuotaObservation>,
    ) -> BoxFuture<'a, ()> { ... }
    fn wait<'a>(&'a self, duration: Duration) -> BoxFuture<'a, ()>;
    fn surface_usage<'a>(
        &'a self,
        identity: &'a CallerIdentity,
        provider: &'a ProviderRef,
        credential: CredentialId,
    ) -> Box<dyn UsageView + 'a>;
    fn spawner(&self) -> Option<&dyn Spawner> { ... }
    fn bindings(&self) -> Option<&dyn BindingStore> { ... }
    fn oauth(&self) -> Option<&dyn gproxy_channel_api::OAuthService> { ... }
    fn continuations(&self) -> Option<&dyn ContinuationStore> { ... }
}
```

`Host` 是交给 `Core::new` 的聚合体；关联类型让每个服务保持静态分派。
`authenticate` 把规范化后的请求映射为 `CallerIdentity`（用户、密钥、组织、
团队）或返回 `CoreError::Unauthorized`。`admit` 应用权限、限流和配额预扣；返
回错误时不得留下任何预留。`finish_admission` 以 `Some(settlement)` 对账，或
以 `None` 退还。`admit_credential` 是按凭证的 RPM/TPM 闸门。`count_tokens`
是宿主的分词器阶梯。`wait` 是宿主为有界服务面轮询提供的计时器；核心库从不
选择执行器。`surface_usage` 把 `UsageView` 借给服务面合成器。四个可选访问器
默认返回 `None`；配额观察器默认丢弃观察值。

### `CredentialStore`（必需）

```rust
pub struct CredentialRecord {
    pub id: CredentialId,
    pub channel: String,
    pub kind: String,
    pub secret: serde_json::Value,
    pub version: u64,
}

pub trait CredentialStore {
    fn load<'a>(&'a self, id: CredentialId) -> BoxFuture<'a, Result<CredentialRecord, StoreError>>;

    fn persist_rotation<'a>(
        &'a self,
        id: CredentialId,
        secret: serde_json::Value,
        version: u64,
    ) -> BoxFuture<'a, Result<(), StoreError>>;

    fn lease_refresh<'a>(
        &'a self,
        id: CredentialId,
        ttl: Duration,
    ) -> BoxFuture<'a, Result<bool, StoreError>>;
}
```

核心库收到的是通道文档规定 JSON 形态的已解密秘密；解密是存储实现的职责，核
心库从不接触密文。`persist_rotation` 必须原子并受 `version` 保护：过期的版
本必须失败而不是覆盖。Claude 每次刷新都会轮换 refresh token，丢失这次写入会
让凭证报废，这正是该 trait 不可选的原因。`lease_refresh` 返回调用方是否持有
独占租约，使并发请求只刷新一次。

### `CacheBackend`

```rust
pub trait CacheBackend {
    fn get<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<Option<Vec<u8>>, StoreError>>;
    fn set<'a>(
        &'a self,
        key: &'a str,
        value: Vec<u8>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<(), StoreError>>;
    fn delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<(), StoreError>>;
    fn incr<'a>(
        &'a self,
        key: &'a str,
        by: i64,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<i64, StoreError>>;
    fn compare_incr_and_set<'a>(
        &'a self,
        counter_key: &'a str,
        by: i64,
        state_key: &'a str,
        expected_state: Vec<u8>,
        state: Vec<u8>,
    ) -> BoxFuture<'a, Result<Option<i64>, StoreError>>;
    fn compare_and_swap<'a>(
        &'a self,
        key: &'a str,
        expected: Option<Vec<u8>>,
        value: Option<Vec<u8>>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<bool, StoreError>>;
}
```

`incr` 对不存在的键从零开始并按 `ttl` 设定过期；对已存在的键递增从不改变
其过期时间。`compare_incr_and_set` 必须两次写入同时提交或同时不提交——配额
对账依赖于此。`compare_and_swap` 支撑长期的所有权租约。共享同一数据库的所
有实例必须共享缓存；`gproxy-store` 提供进程内、Redis、Upstash 和 libSQL 实
现。

### `UpstreamTransport`

```rust
pub trait UpstreamTransport: MaybeSync {
    fn send<'a>(
        &'a self,
        request: http::Request<bytes::Bytes>,
    ) -> BoxFuture<'a, Result<http::Response<crate::boundary::ByteStream>, TransportError>>;

    fn open_websocket<'a>(
        &'a self,
        request: http::Request<bytes::Bytes>,
    ) -> BoxFuture<'a, Result<Box<dyn WsDuplex>, TransportError>>;
}
```

请求体是缓冲的 `Bytes`，因为转换和重试需要重放；响应以流返回。
`gproxy-upstream` 是规范实现，嵌入方也可以自带。

### `UsageSink` 与 `CaptureSink`

```rust
pub trait UsageSink {
    fn record<'a>(&'a self, settlement: &'a Settlement) -> BoxFuture<'a, ()>;
}

pub trait CaptureSink {
    fn record<'a>(&'a self, capture: &'a Capture) -> BoxFuture<'a, ()>;
}
```

漏斗提供每一次结算和每一次捕获的交换；由 sink 决定保留和脱敏。`gproxy-app`
写入用量记录和线路日志；嵌入方可以在内存中聚合，也可以丢弃。两者都不在热
路径的关键区内运行。

### `Spawner`

```rust
pub trait Spawner {
    #[cfg(not(target_arch = "wasm32"))]
    fn spawn(&self, task: std::pin::Pin<Box<dyn Future<Output = ()> + Send>>);
    #[cfg(target_arch = "wasm32")]
    fn spawn(&self, task: std::pin::Pin<Box<dyn Future<Output = ()>>>);
}
```

可选。宿主提供时，流式结算在响应结束后分离执行；不提供时，结算在流关闭前
内联完成。没有 spawner 的宿主必须在客户端断开后继续拉取上游流，并显式关闭
桥接的 websocket，因为 `Drop` 无法等待异步 sink。

### `BindingStore`

```rust
pub trait BindingStore: MaybeSync {
    fn save<'a>(
        &'a self,
        provider_id: i64,
        owner_user_id: i64,
        kind: &'static str,
        id: &'a str,
        credential: CredentialId,
        summary: Value,
    ) -> BoxFuture<'a, Result<(), StateError>>;
    fn find<'a>(
        &'a self,
        provider_id: i64,
        owner_user_id: i64,
        kind: &'static str,
        id: &'a str,
    ) -> BoxFuture<'a, Result<Option<Binding>, StateError>>;
    fn delete<'a>(
        &'a self,
        provider_id: i64,
        owner_user_id: i64,
        kind: &'static str,
        id: &'a str,
    ) -> BoxFuture<'a, Result<(), StateError>>;
    // ... 以及分页的 `list`
}
```

为有状态服务面和资源亲和操作（文件、视频、realtime 呼叫）提供持久的资源到
凭证绑定。刻意不提供内存默认实现：绑定必须跨重启存活并在实例间共享。返回
`None` 的宿主不能注册声明了服务面表的通道；`Core::new` 会直接失败。

### `ControlPlane`

```rust
pub trait ControlPlane: gproxy_channel_api::MaybeSend + gproxy_channel_api::MaybeSync {
    fn resolve_alias(&self, model: &str, mode: &RoutingMode) -> String { ... }
    fn resolve(
        &self,
        model: Option<&str>,
        mode: &RoutingMode,
        affinity: Option<i64>,
    ) -> Result<Plan, CoreError> { ... }
    fn resolve_variant(&self, model: &str, mode: &RoutingMode) -> Option<String> { ... }
    fn resolve_preprocessed(
        &self,
        model: Option<&str>,
        mode: &RoutingMode,
        affinity: Option<i64>,
    ) -> Result<Plan, CoreError>;
    fn pricing(&self, provider: &ProviderRef, upstream_model: &str) -> Option<Pricing>;
    fn exposed_models(&self) -> Vec<ExposedModel>;
    fn provider_catalogue(&self) -> Vec<ExposedModel> { ... }
    fn detached(&self) -> Box<dyn ControlPlane>;
}

pub struct Plan {
    pub targets: Vec<Target>,
    pub budget: FailoverBudget,
}

pub struct Target {
    pub provider: ProviderRef,
    pub credential: CredentialId,
    pub upstream_model: String,
    pub tier: u32,
    pub rules: TargetRules,
}

pub struct FailoverBudget {
    pub max_attempts: u32,
}
```

读模型刻意设计为同步：实现从内存快照回答，从不在热路径上做 I/O。带默认实
现的 `resolve` 依次应用 `resolve_alias`、`resolve_variant`、
`resolve_preprocessed`。`gproxy-app` 基于 ArcSwap 快照实现该 trait；嵌入方
可以基于静态配置实现，或者跳过它、为 `execute_planned` 自行构造 `Plan`。
`pricing` 返回 `None` 时以零成本结算并打印警告，而不是拒绝请求。

`OAuthService`（模拟厂商认证面的签发方状态）和 `ContinuationStore`（对活动
上游流的进程内所有权，仅原生）是可选的；需要它们的通道在宿主返回 `None` 时
被 `Core::new` 拒绝。

## 边界类型

来自 `crates/gproxy-core/src/boundary.rs` 和 `usage.rs`：

```rust
pub struct RequestCtx {
    pub request_id: String,
    pub client_ip: Option<std::net::IpAddr>,
    pub method: Method,
    pub path: String,
    pub query: Option<String>,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub upgrade: bool,
    pub mode: RoutingMode,
}

pub enum RoutingMode {
    Aggregated,
    Namespace { namespace: String },
    Scoped { provider: String },
    Named { name: String },
}

pub struct ExecOutcome {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: ResponseBody,
    pub disposition: Disposition,
    // private settlement proof
}

pub enum ResponseBody {
    Full(Bytes),
    Stream(ByteStream),
    WebSocket(Box<dyn gproxy_channel_api::WsDuplex>),
}

pub type ByteStream =
    std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, TransportError>> + Send>>;

pub struct Settlement {
    pub request_id: String,
    pub provider_id: i64,
    pub credential_id: crate::host::CredentialId,
    pub upstream_model: String,
    pub usage: NormalizedUsage,
    pub cost: Decimal,
    pub source: UsageSource,
    pub ended: Ended,
    pub latency_ms: u64,
}
```

`request_id` 由宿主分配，贯穿每条日志、捕获行和用量记录。请求体是缓冲的
`Bytes`，克隆零成本，重试可重放。`mode` 来自
`gproxy_app::ingress::normalize_path` 或你自己的前缀处理；`Named` 由控制面
落定为命名空间、路由或 Provider。`ExecOutcome` 无法在核心库之外构造：私有
的结算凭据意味着宿主渲染的每个结果都经过了漏斗。`CoreError::status()` 和
`CoreError::body_json()` 让每个宿主渲染相同的状态码和 OpenAI 风格的错误信
封：401 未认证，403 禁止，404 未知路由或 Provider，400 不支持，429 限流，
402 配额超限，502 对应无凭证、上游全部失败和传输失败，500 对应转换、存储和
内部错误。

## wasm 说明

- `BoxFuture`、`ByteStream` 和 `Spawner::spawn` 在 `wasm32` 上去掉 `Send`；
  `MaybeSend`/`MaybeSync` 变为空标记。同一份 trait 定义服务两个目标。
- 共享句柄在 wasm 上是 `Rc`，原生上是 `Arc`。
- 没有 `Spawner` 时，结算在响应流结束前内联运行。Edge 宿主通过平台的
  `waitUntil` 续体保持流存活。
- 需要 `ContinuationStore` 的通道仅原生可用，在没有它的宿主上被 `Core::new`
  拒绝。
- wasm 上的持久化是基于 fetch 的 libSQL；缓存是 libSQL 表或 Upstash。

## 参考嵌入方：`gproxy-app`

```rust
impl App {
    pub async fn start(config: Config) -> Result<AppHandle, AppError>
}

impl AppHandle {
    pub async fn execute(
        &self,
        request: RequestCtx,
    ) -> Result<ExecOutcome, gproxy_core::CoreError>
    pub async fn admin_dispatch(
        &self,
        parts: &http::request::Parts,
        body: bytes::Bytes,
    ) -> Option<http::Response<bytes::Bytes>>
    pub async fn portal_dispatch(
        &self,
        parts: &http::request::Parts,
        body: bytes::Bytes,
    ) -> Option<http::Response<bytes::Bytes>>
    pub async fn mutate(
        &self,
        mutation: crate::ControlMutation,
    ) -> Result<crate::MutationResult, AppError>
    pub async fn reload(&self) -> Result<(), AppError>
    pub fn shutdown(&self)
    pub async fn wait_shutdown(&self)
}
```

`App::start` 打开存储并迁移，在全新存储上播种内置价格目录，准备主密钥加密
器（武装时执行轮换），播种首次运行的管理员，构建快照控制面、缓存、传输和
分词器注册表，调用 `Core::new(host, channels)`，并安排清理任务。`Config` 在
原生上来自 `Config::from_env()` 或 `NativeCommand::from_env()`，在 wasm 上来
自 `Config::libsql(url, auth_token, MasterKeyConfig)`，在代码中也可用
`Config::sqlite(listen_addr, data_dir, MasterKeyConfig)` 构造。
`gproxy_app::ingress::decode_body` 和 `normalize_path` 是公开的，宿主可以用
它们做同样的请求体解码和命名前缀处理。两个宿主分别是
`gproxy_host_axum::AxumServer::bind_with_config(app, address, HostConfig)`
和 `gproxy_host_edge::start(EdgeConfig)`。

## 可独立使用的库

**`gproxy-protocol`**：`match_ingress_for(method, path, preferred)`、
`Operation::spec()`、`registered_operations()`，以及 `openai`、`claude`、
`gemini` 和 `aws` 下的线路模型。每个模型都把未建模的字段保留在扁平化的
`rest` 中，因此往返转换不会丢弃上游添加的内容。对外部使用者，枚举是
`#[non_exhaustive]`；`exhaustive` feature 会把新增变体变成一份编译错误清单。

**`gproxy-transform`**：

```rust
pub fn can_transform(source: OperationKey, target: OperationKey) -> bool

pub fn request(
    source: OperationKey,
    target: OperationKey,
    body: Bytes,
    upstream_model: &str,
    stream: bool,
) -> Result<Bytes, TransformError>

pub fn response(
    source: OperationKey,
    target: OperationKey,
    body: Bytes,
) -> Result<Bytes, TransformError>

pub fn response_stream(
    source: OperationKey,
    target: OperationKey,
) -> Result<ResponseStream, TransformError>

pub fn response_stream_framed(
    source: OperationKey,
    target: OperationKey,
    source_framing: StreamFraming,
    target_framing: StreamFraming,
) -> Result<ResponseStream, TransformError>
```

转换对是显式声明的，没有中间格式：Chat ↔ Claude、Responses ↔ Claude、
Claude ↔ Gemini、Gemini ↔ Chat、Gemini ↔ Responses、Chat ↔ Responses，模型
列表和 token 统计的 OpenAI ↔ Claude，以及 Compact → Claude，另有 WebSocket
上的 Responses 等封装提升。全部同步、无 I/O。

**`gproxy-tokenize`**：`count(model, body, map, registry) -> u64`、
`count_detailed`、`count_text`、`try_count` 和 `harvest`，由 feature
`tiktoken`、`hf-registry` 和 `bundled-fallback` 控制。它实现的阶梯见
[价格与分层](/zh-cn/reference/pricing/)。
