# gproxy-provider

`gproxy-provider` 是 SDK 层的 provider 引擎。它用 `Channel` trait 统一不同上游渠道，用 `ProviderStore` 管理 provider 与 credential 状态，再由 `GproxyEngine` 执行请求，并在底层通过 `retry_with_credentials` 完成凭证轮换与重试。

## 架构概览

| 层 | 公开入口 | 作用 |
| --- | --- | --- |
| 渠道抽象 | `channel::Channel` | 定义某个上游渠道的 dispatch、鉴权、请求构造、响应分类和可选 OAuth / quota / WS 行为。 |
| Provider 运行时 | `store::ProviderStore` | 保存 provider 实例、credential 快照、健康状态与事件流。 |
| 执行引擎 | `engine::GproxyEngine` | 接收 `ExecuteRequest`，从 store 取 provider，完成 transform、执行和计费估算。 |
| 重试内核 | `retry::retry_with_credentials` / `retry::retry_with_credentials_stream` | 在多个 credential 之间做轮换、冷却、401/403 刷新与 429 重试。 |

## 调用链

1. `GproxyEngine::execute` 接收 `ExecuteRequest`。
2. `GproxyEngine` 从 `ProviderStore` 取出 provider runtime。
3. runtime 使用 `Channel::dispatch_table`、`finalize_request`、`normalize_response` 等逻辑处理协议与请求。
4. runtime 调用 `retry_with_credentials` 或 `retry_with_credentials_stream`。
5. 重试内核通过 `Channel::prepare_request` 发起上游请求，并基于 `Channel::classify_response`、`CredentialHealth` 和 `forced_credential` 等信息决定是否切换凭证或重试。
6. `GproxyEngine` 产出 `ExecuteResult`，其中可包含 `usage`、`billing`、`meta` 和 `credential_updates`。

## 内置渠道

`src/channels/` 当前一共有 14 个渠道模块：

| 渠道 ID | Channel 类型 | Settings 类型 | Credential 类型 | Cargo feature |
| --- | --- | --- | --- | --- |
| `openai` | `OpenAiChannel` | `OpenAiSettings` | `OpenAiCredential` | `openai` |
| `anthropic` | `AnthropicChannel` | `AnthropicSettings` | `AnthropicCredential` | `anthropic` |
| `aistudio` | `AiStudioChannel` | `AiStudioSettings` | `AiStudioCredential` | `aistudio` |
| `vertexexpress` | `VertexExpressChannel` | `VertexExpressSettings` | `VertexExpressCredential` | `vertexexpress` |
| `vertex` | `VertexChannel` | `VertexSettings` | `VertexCredential` | `vertex` |
| `geminicli` | `GeminiCliChannel` | `GeminiCliSettings` | `GeminiCliCredential` | `geminicli` |
| `claudecode` | `ClaudeCodeChannel` | `ClaudeCodeSettings` | `ClaudeCodeCredential` | `claudecode` |
| `codex` | `CodexChannel` | `CodexSettings` | `CodexCredential` | `codex` |
| `antigravity` | `AntigravityChannel` | `AntigravitySettings` | `AntigravityCredential` | `antigravity` |
| `nvidia` | `NvidiaChannel` | `NvidiaSettings` | `NvidiaCredential` | `nvidia` |
| `deepseek` | `DeepSeekChannel` | `DeepSeekSettings` | `DeepSeekCredential` | `deepseek` |
| `groq` | `GroqChannel` | `GroqSettings` | `GroqCredential` | `groq` |
| `openrouter` | `OpenRouterChannel` | `OpenRouterSettings` | `OpenRouterCredential` | `openrouter` |
| `custom` | `CustomChannel` | `CustomSettings` | `CustomCredential` | `custom` |

## 核心 trait

### `Channel`

| 项 | 签名 | 说明 |
| --- | --- | --- |
| trait 头 | `pub trait Channel: Send + Sync + 'static` | 渠道统一抽象。 |
| 常量 | `const ID: &'static str;` | 渠道唯一 ID。 |
| 关联类型 | `type Settings: ChannelSettings;` | 渠道配置。 |
| 关联类型 | `type Credential: ChannelCredential;` | 渠道凭证。 |
| 关联类型 | `type Health: CredentialHealth;` | 渠道健康状态类型。 |
| 必选方法 | `fn dispatch_table(&self) -> DispatchTable;` | 返回默认 dispatch 表。 |
| 必选方法 | `fn prepare_request(&self, credential: &Self::Credential, settings: &Self::Settings, request: &PreparedRequest) -> Result<http::Request<Vec<u8>>, UpstreamError>;` | 组装上游 HTTP 请求。 |
| 必选方法 | `fn classify_response(&self, status: u16, headers: &http::HeaderMap, body: &[u8]) -> ResponseClassification;` | 分类响应，决定是否重试。 |
| 默认方法 | `fn model_pricing(&self) -> &'static [crate::billing::ModelPrice]` | 提供渠道自带定价表。 |
| 默认方法 | `fn finalize_request(&self, _settings: &Self::Settings, request: PreparedRequest) -> Result<PreparedRequest, UpstreamError>` | 在 transport 封装前做语义级归一化。 |
| 默认方法 | `fn normalize_response(&self, _request: &PreparedRequest, body: Vec<u8>) -> Vec<u8>` | 归一化上游响应 body。 |
| 默认方法 | `fn count_strategy(&self) -> crate::count_tokens::CountStrategy` | 返回 token 计数策略。 |
| 默认方法 | `fn handle_local(&self, _operation: OperationFamily, _protocol: ProtocolKind, _body: &[u8]) -> Option<Result<Vec<u8>, UpstreamError>>` | 处理本地路由。 |
| 默认方法 | `fn needs_spoof_client(&self, _credential: &Self::Credential) -> bool` | 是否需要 spoof client。 |
| 默认方法 | `fn ws_extra_headers(&self) -> http::HeaderMap` | WebSocket 握手附加 Header。 |
| 默认方法 | `fn model_suffix_groups(&self) -> &'static [SuffixGroup]` | 额外模型后缀组。 |
| 默认方法 | `fn refresh_credential<'a>(&'a self, _client: &'a wreq::Client, _credential: &'a mut Self::Credential) -> impl Future<Output = Result<bool, UpstreamError>> + Send + 'a` | 401/403 后尝试刷新 credential。 |
| 默认方法 | `fn prepare_quota_request(&self, _credential: &Self::Credential, _settings: &Self::Settings) -> Result<Option<http::Request<Vec<u8>>>, UpstreamError>` | 构造 quota 查询请求。 |
| 默认方法 | `fn oauth_start<'a>(&'a self, _client: &'a wreq::Client, _settings: &'a Self::Settings, _params: &'a BTreeMap<String, String>) -> OAuthFuture<'a, OAuthFlow>` | 启动 OAuth。 |
| 默认方法 | `fn oauth_finish<'a>(&'a self, _client: &'a wreq::Client, _settings: &'a Self::Settings, _params: &'a BTreeMap<String, String>) -> OAuthFuture<'a, OAuthCredentialResult<Self::Credential>>` | 完成 OAuth。 |

### `ChannelSettings`

| 项 | 签名 | 说明 |
| --- | --- | --- |
| trait 头 | `pub trait ChannelSettings: Send + Sync + Clone + Default + Serialize + DeserializeOwned + 'static` | 渠道配置约束。 |
| 必选方法 | `fn base_url(&self) -> &str;` | 返回渠道基础 URL。 |
| 默认方法 | `fn user_agent(&self) -> Option<&str>` | 可选 User-Agent。 |
| 默认方法 | `fn max_retries_on_429(&self) -> u32` | 429 的每凭证最大重试次数。 |
| 默认方法 | `fn enable_cache_affinity(&self) -> bool` | 是否启用 cache affinity。 |

### `ChannelCredential`

| 项 | 签名 | 说明 |
| --- | --- | --- |
| trait 头 | `pub trait ChannelCredential: Send + Sync + Clone + Serialize + DeserializeOwned + 'static` | 渠道凭证约束。 |
| 默认方法 | `fn apply_update(&mut self, _update: &serde_json::Value) -> bool` | 应用上游返回的 credential 更新。 |

### `CredentialHealth`

| 项 | 签名 | 说明 |
| --- | --- | --- |
| trait 头 | `pub trait CredentialHealth: Send + Sync + Clone + Default + 'static` | 凭证健康状态抽象。 |
| 方法 | `fn is_available(&self, model: Option<&str>) -> bool;` | 判断某模型下凭证是否可用。 |
| 方法 | `fn record_error(&mut self, status: u16, model: Option<&str>, retry_after_ms: Option<u64>);` | 记录失败。 |
| 方法 | `fn record_success(&mut self, model: Option<&str>);` | 记录成功。 |

### Backend traits

| trait | 签名 | 说明 |
| --- | --- | --- |
| `RateLimitBackend` | `pub trait RateLimitBackend: Send + Sync + 'static` | 分布式或本地限流计数后端。 |
| `RateLimitBackend::try_acquire` | `fn try_acquire(&self, key: &str, window: RateLimitWindow) -> impl Future<Output = Result<u64, RateLimitExceeded>> + Send;` | 消耗一个窗口内请求额度。 |
| `RateLimitBackend::current_count` | `fn current_count(&self, key: &str, window: RateLimitWindow) -> impl Future<Output = u64> + Send;` | 读取当前窗口计数。 |
| `QuotaBackend` | `pub trait QuotaBackend: Send + Sync + 'static` | 配额预占与结算后端。 |
| `QuotaBackend::try_reserve` | `fn try_reserve(&self, identity_id: i64, estimated_cost: u64) -> impl Future<Output = Result<Self::Hold, QuotaExhausted>> + Send;` | 预占配额。 |
| `QuotaBackend::balance` | `fn balance(&self, identity_id: i64) -> impl Future<Output = Result<QuotaBalance, QuotaError>> + Send;` | 查询配额余额。 |
| `QuotaBackend::set_quota` | `fn set_quota(&self, identity_id: i64, total: u64) -> impl Future<Output = Result<(), QuotaError>> + Send;` | 设置总配额。 |
| `QuotaHold` | `pub trait QuotaHold: Send + 'static` | 配额预占句柄。 |
| `QuotaHold::settle` | `fn settle(self, actual_cost: u64) -> impl Future<Output = Result<(), QuotaError>> + Send;` | 用真实成本结算。 |
| `AffinityBackend` | `pub trait AffinityBackend: Send + Sync + 'static` | 凭证 affinity 绑定后端。 |
| `AffinityBackend::get_binding` | `fn get_binding(&self, key: &str) -> impl Future<Output = Option<String>> + Send;` | 读取绑定。 |
| `AffinityBackend::set_binding` | `fn set_binding(&self, key: &str, credential_id: &str, ttl: Duration) -> impl Future<Output = Result<(), BackendError>> + Send;` | 写入绑定。 |
| `AffinityBackend::remove_binding` | `fn remove_binding(&self, key: &str) -> impl Future<Output = Result<(), BackendError>> + Send;` | 删除绑定。 |

## `GproxyEngine` 使用示例

```rust
use gproxy_provider::{
    GproxyEngine,
    channels::openai::{OpenAiChannel, OpenAiCredential, OpenAiSettings},
    health::ModelCooldownHealth,
};

let engine = GproxyEngine::builder()
    .add_provider(
        "openai-main",
        OpenAiChannel,
        OpenAiSettings::default(),
        vec![(
            OpenAiCredential {
                api_key: std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY"),
            },
            ModelCooldownHealth::default(),
        )],
    )
    .enable_usage(true)
    .enable_upstream_log(true)
    .enable_upstream_log_body(false)
    .build();

let store = engine.store().clone();
let providers = store.list_providers().unwrap();
assert_eq!(providers[0].name, "openai-main");
```

`GproxyEngineBuilder` 的主要公开方法如下：

| 方法 | 签名 | 说明 |
| --- | --- | --- |
| `new` | `fn new() -> Self` | 创建 builder。 |
| `provider_store` | `fn provider_store(mut self, store: Arc<ProviderStore>) -> Self` | 复用现有 store。 |
| `add_provider` | `fn add_provider<C: crate::Channel>(mut self, name: impl Into<String>, channel: C, settings: C::Settings, credentials: Vec<(C::Credential, C::Health)>) -> Self` | 直接注册 provider。 |
| `http_client` | `fn http_client(mut self, client: wreq::Client) -> Self` | 指定普通 HTTP client。 |
| `spoof_client` | `fn spoof_client(mut self, client: wreq::Client) -> Self` | 指定 spoof HTTP client。 |
| `configure_clients` | `fn configure_clients(self, proxy: Option<&str>, emulation: Option<&str>) -> Self` | 通过 proxy / emulation 构造 client。 |
| `enable_usage` | `fn enable_usage(mut self, enabled: bool) -> Self` | 控制 usage 抽取。 |
| `enable_upstream_log` | `fn enable_upstream_log(mut self, enabled: bool) -> Self` | 控制上游请求日志。 |
| `enable_upstream_log_body` | `fn enable_upstream_log_body(mut self, enabled: bool) -> Self` | 控制上游 body 日志。 |
| `build` | `fn build(self) -> GproxyEngine` | 构建引擎。 |
| `add_provider_json` | `fn add_provider_json(self, config: ProviderConfig) -> Result<Self, UpstreamError>` | 通过 JSON 配置装配 provider。 |

## `ProviderStore` API

### `ProviderRegistry`

| 方法 | 签名 | 说明 |
| --- | --- | --- |
| `get_provider` | `fn get_provider(&self, name: &str) -> Result<Option<ProviderSnapshot>, UpstreamError>;` | 按名称读取 provider 快照。 |
| `list_providers` | `fn list_providers(&self) -> Result<Vec<ProviderSnapshot>, UpstreamError>;` | 列出所有 provider。 |
| `get_credential` | `fn get_credential(&self, provider_name: &str, index: usize) -> Result<Option<CredentialSnapshot>, UpstreamError>;` | 读取指定 credential。 |
| `list_credentials` | `fn list_credentials(&self, provider_name: Option<&str>) -> Result<Vec<CredentialSnapshot>, UpstreamError>;` | 列出 credential。 |

### `ProviderMutator`

| 方法 | 签名 | 说明 |
| --- | --- | --- |
| `upsert_provider_json` | `fn upsert_provider_json(&self, config: crate::engine::ProviderConfig) -> Result<(), UpstreamError>;` | 新增或替换 provider。 |
| `remove_provider` | `fn remove_provider(&self, name: &str) -> bool;` | 删除 provider。 |
| `upsert_credential_json` | `fn upsert_credential_json(&self, provider_name: &str, index: Option<usize>, credential: Value) -> Result<Option<CredentialSnapshot>, UpstreamError>;` | 新增或更新 credential。 |
| `remove_credential` | `fn remove_credential(&self, provider_name: &str, index: usize) -> Result<Option<CredentialSnapshot>, UpstreamError>;` | 删除 credential。 |

### `EngineEventSource`

| 方法 | 签名 | 说明 |
| --- | --- | --- |
| `subscribe` | `fn subscribe(&self) -> broadcast::Receiver<EngineEvent>;` | 订阅 provider / credential 事件。 |

### `EngineEvent`

| 事件 | 负载 | 说明 |
| --- | --- | --- |
| `ProviderAdded` | `{ name: String }` | 新增 provider。 |
| `ProviderRemoved` | `{ name: String }` | 删除 provider。 |
| `ProviderUpdated` | `{ name: String }` | provider 配置或 credential 有变更。 |
| `CredentialHealthChanged` | `{ provider: String, index: usize, status: String }` | credential 健康状态发生变化。 |

### `ProviderStore` 额外公开方法

| 方法 | 签名 | 说明 |
| --- | --- | --- |
| `builder` | `fn builder() -> ProviderStoreBuilder` | 创建 `ProviderStoreBuilder`。 |
| `add_provider` | `fn add_provider<C: Channel>(&self, name: impl Into<String>, channel: C, settings: C::Settings, credentials: Vec<(C::Credential, C::Health)>)` | 直接加入 provider。 |
| `add_provider_json` | `fn add_provider_json(&self, config: crate::engine::ProviderConfig) -> Result<(), UpstreamError>` | 通过 JSON 配置加入 provider。 |
| `list_health` | `fn list_health(&self, provider_name: Option<&str>) -> Vec<CredentialHealthSnapshot>` | 读取健康快照。 |
| `mark_credential_dead` | `fn mark_credential_dead(&self, provider_name: &str, index: usize) -> bool` | 手工标记 credential 失效。 |
| `mark_credential_healthy` | `fn mark_credential_healthy(&self, provider_name: &str, index: usize) -> bool` | 手工重置 credential 为健康。 |
| `update_provider_settings` | `fn update_provider_settings(&self, provider_name: &str, settings: Value) -> Result<bool, UpstreamError>` | 更新 provider settings。 |
| `add_credential` | `fn add_credential(&self, provider_name: &str, credential: Value) -> Result<Option<CredentialSnapshot>, UpstreamError>` | 新增 credential。 |
| `update_credential` | `fn update_credential(&self, provider_name: &str, index: usize, credential: Value) -> Result<Option<CredentialSnapshot>, UpstreamError>` | 更新 credential。 |
| `remove_credential` | `fn remove_credential(&self, provider_name: &str, index: usize) -> Result<Option<CredentialSnapshot>, UpstreamError>` | 删除 credential。 |
| `apply_credential_update` | `fn apply_credential_update(&self, update: &CredentialUpdate) -> Result<bool, UpstreamError>` | 应用单条 credential 更新。 |
| `apply_credential_updates` | `fn apply_credential_updates(&self, updates: &[CredentialUpdate]) -> Result<Vec<bool>, UpstreamError>` | 批量应用 credential 更新。 |
| `oauth_start` | `async fn oauth_start(&self, provider_name: &str, client: &wreq::Client, params: HashMap<String, String>) -> Result<Option<OAuthFlow>, UpstreamError>` | 启动 OAuth。 |
| `oauth_finish` | `async fn oauth_finish(&self, provider_name: &str, client: &wreq::Client, params: HashMap<String, String>) -> Result<Option<OAuthFinishResult>, UpstreamError>` | 完成 OAuth 并落入 credential。 |
| `get_dispatch_table` | `fn get_dispatch_table(&self, name: &str) -> Option<DispatchTable>` | 读取 provider dispatch 表。 |
| `estimate_billing` | `fn estimate_billing(&self, provider_name: &str, context: &crate::billing::BillingContext, usage: &crate::engine::Usage) -> Option<crate::billing::BillingResult>` | 基于 usage 估算账单。 |

## Backend 抽象与 InMemory 实现

`backend` 模块当前分成 `memory`、`traits` 和 `types` 三部分。

| 抽象/实现 | 公开项 | 说明 |
| --- | --- | --- |
| 限流后端 | `RateLimitBackend` | 抽象限流计数。 |
| 限流内存实现 | `InMemoryRateLimit` | 提供 `new()` 和 `purge_expired()`，适合单进程或测试环境。 |
| 配额后端 | `QuotaBackend`, `QuotaHold` | 抽象配额预占与结算。 |
| 配额内存实现 | `InMemoryQuota`, `InMemoryQuotaHold` | 提供 `new()`，适合单进程或测试环境。 |
| Affinity 后端 | `AffinityBackend` | 抽象 credential 绑定。 |
| Affinity 内存实现 | `InMemoryAffinity` | 提供 `new()`，用内存保存 TTL 绑定。 |
| 共享类型 | `RateLimitWindow`, `RateLimitExceeded`, `QuotaBalance`, `QuotaExhausted`, `QuotaError`, `BackendError` | backend 层通用数据与错误类型。 |

说明：

- 这些 InMemory 类型已经在 crate 根重新导出：`InMemoryAffinity`、`InMemoryQuota`、`InMemoryRateLimit`。
- 当前 `gproxy-provider` 没有 Redis backend 实现，也没有 `redis` feature flag。

## 如何添加新渠道

1. 在 `src/channels/` 下新增一个渠道模块，定义 `XxxChannel`、`XxxSettings` 和 `XxxCredential`。
2. 为新渠道实现 `Channel`，并让 `XxxSettings` 实现 `ChannelSettings`、`XxxCredential` 实现 `ChannelCredential`，同时选定 `type Health: CredentialHealth`。
3. 写出渠道默认 dispatch 表，并用 `inventory::submit! { ChannelRegistration::new(XxxChannel::ID, xxx_dispatch_table) }` 注册。
4. 在 `src/channels/mod.rs` 中公开该模块。
5. 如果希望字符串渠道 ID 也能走 JSON 装配路径，还需要把新渠道加入 `engine.rs` 里的 `validate_credential_json`、`GproxyEngineBuilder::add_provider_json`，以及 `store.rs` 里的 `ProviderStore::add_provider_json` 三处 `match` 分派。
