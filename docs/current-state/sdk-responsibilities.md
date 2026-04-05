# SDK 各 Crate 当前职责盘点

> 基于 `sdk/` 目录下实际代码整理，截至 2026-04-05。

---

## 总体结构

```
sdk/
├── gproxy-sdk/           主 crate，纯 re-export
├── gproxy-protocol/      wire format 类型 + 跨协议转换
└── gproxy-provider/      渠道引擎（执行、重试、健康、状态管理）
```

SDK 与 App 层（`crates/*`）之间**零编译依赖**——SDK 不 import 任何 `crates/` 代码。依赖方向是单向的：App → SDK。

---

## 1. `gproxy-sdk`

**源码：** `sdk/gproxy-sdk/src/lib.rs`（2 行）

```rust
pub use gproxy_protocol as protocol;
pub use gproxy_provider as provider;
```

### 当前职责

- 纯 re-export 入口。下游 `cargo add gproxy-sdk` 即可获得 `protocol` 和 `provider` 两个命名空间。
- 不包含任何自有逻辑、类型定义或 trait。

### 不负责的

- 不做任何组装、初始化或配置。
- 不承担运行时状态。

---

## 2. `gproxy-protocol`

**源码：** `sdk/gproxy-protocol/src/`

### 当前职责

纯类型层。提供三种 LLM API 协议的 wire format 结构体和跨协议转换。

#### 模块构成

| 模块 | 内容 |
|------|------|
| `claude/` | CreateMessage、CountTokens、ModelList/Get、File（Upload/Get/Download/Delete/List）的请求/响应/流事件类型 |
| `openai/` | ChatCompletions、Responses（含 WebSocket 子模块）、CompactResponse、Embeddings、CreateImage、ModelList/Get |
| `gemini/` | GenerateContent（含 BidiGenerateContent）、Embeddings、VideoGeneration、ModelList/Get |
| `stream.rs` | `SseToNdjsonRewriter` — 增量 SSE→NDJSON 字节转换器 |
| `transform.rs` | 基于 `TryFrom` 的跨协议转换分发（OpenAI↔Claude↔Gemini 请求/响应互转） |
| `lib.rs` | `OperationFamily` 和 `ProtocolKind` 枚举 — 统一标识操作类型和协议种类 |

#### 每个操作的标准文件结构

```
{operation}/
  mod.rs       — pub exports
  request.rs   — 请求结构体
  response.rs  — 响应结构体
  stream.rs    — 流事件 payload（仅 JSON 结构，不含 SSE 帧）
  types.rs     — 共享枚举/子类型
```

### 对外暴露的核心能力

- `pub mod claude / openai / gemini` — 协议特定类型
- `pub mod kinds::{OperationFamily, ProtocolKind}` — 操作/协议分类
- `pub mod stream::SseToNdjsonRewriter` — 流格式转换
- `pub mod transform` — 跨协议 TryFrom 实现

### 纯库特性

- 仅依赖 `serde`、`serde_json`、`base64`、`time`
- **零 I/O、零状态、零副作用**
- 与 App 层无耦合

### 不负责的

- 不做请求分类（`classify` 在 `gproxy-server/middleware` 中）
- 不做 HTTP 请求构建或发送
- 包含 `SseToNdjsonRewriter`（SSE 帧解析 + NDJSON 转换），但不负责流的传输层（chunked read、WebSocket framing 等）
- 不承担任何运行时或 App 层职责

---

## 3. `gproxy-provider`

**源码：** `sdk/gproxy-provider/src/`  
这是 SDK 的核心 crate，也是与 App 层关系最密切的部分。

### 3.1 模块总览

| 模块 | 职责分类 | 说明 |
|------|---------|------|
| `engine.rs` | **运行时** | `GproxyEngine` 主执行引擎 |
| `store.rs` | **运行时状态** | `ProviderStore` 线程安全凭证/健康状态管理 |
| `channel.rs` | 纯抽象 | `Channel` / `ChannelSettings` / `ChannelCredential` / `OAuthFlow` trait |
| `registry.rs` | 纯抽象 | `ChannelRegistry` — inventory 自动注册 |
| `dispatch.rs` | 纯数据 | `DispatchTable` / `RouteKey` / `RouteImplementation` 路由表 |
| `retry.rs` | **运行时** | 凭证轮转重试逻辑 |
| `health.rs` | **运行时状态** | `CredentialHealth` trait + `SimpleHealth` / `ModelCooldownHealth` 实现 |
| `affinity.rs` | **运行时状态** | `CacheAffinityPool` prompt-cache 亲和绑定（LRU） |
| `billing.rs` | 纯计算 | 定价模型 + 费用计算 |
| `usage.rs` | 纯提取 | 从响应中提取 token 用量 |
| `request.rs` | 纯数据 | `PreparedRequest` 请求包装 |
| `response.rs` | 纯数据 | `UpstreamResponse` / `ResponseClassification` / `UpstreamError` |
| `transform_dispatch.rs` | 纯转换 | 请求/响应跨协议转换分发 |
| `count_tokens.rs` | 纯计算 | Token 计数策略 |
| `http_client.rs` | I/O | HTTP 发送包装 |
| `provider.rs` | 纯数据 | `ProviderDefinition` 定义 |
| `suffix.rs` | 纯数据 | 模型后缀分组（如 `-1m`、`-200k`） |
| `channels/` | 渠道实现 | 14 个内置渠道 |
| `utils/` | 辅助 | OAuth 工具、请求规范化 |

### 3.2 engine — 执行引擎

**`GproxyEngine` 结构：**

```rust
pub struct GproxyEngine {
    store: Arc<ProviderStore>,
    client: wreq::Client,
    spoof_client: Option<wreq::Client>,
    pub enable_usage: bool,
    pub enable_upstream_log: bool,
    pub enable_upstream_log_body: bool,
}
```

**执行路径：**
1. `execute_request(ExecuteRequest)` → 从 `ProviderStore` 获取 provider runtime
2. 调用 `provider.execute(PreparedRequest, ...)` → 内部走 `retry_with_credentials`
3. 返回 `ExecuteResult { status, headers, body, usage, billing, meta, credential_updates }`

**`ExecuteBody`：** `Full(Vec<u8>)` | `Stream(Pin<Box<dyn Stream>>)` — 支持流式和非流式。

**Builder 模式：** `GproxyEngine::builder().add_provider(...).configure_clients(...).build()`

### 3.3 store — 凭证与健康状态管理

**`ProviderStore` 结构：**

```rust
pub struct ProviderStore {
    providers: ArcSwap<HashMap<String, Arc<dyn ProviderRuntime>>>
}
```

每个 provider 包装为 `ProviderInstance<C: Channel>`，内含：
- `settings: ArcSwap<C::Settings>` — 可热更新
- `credentials: ArcSwap<Vec<C::Credential>>` — 可轮转
- `health: Mutex<Vec<C::Health>>` — 每凭证健康状态
- `affinity_pool: CacheAffinityPool` — prompt-cache 亲和
- `dispatch_table: DispatchTable` — 路由映射
- `credential_revision: AtomicU64` — 乐观锁

**核心 API：**
- `add_provider / list_providers / get_provider` — 生命周期
- `update_provider_settings / add_credential / update_credential / remove_credential` — 热更新
- `apply_credential_update` — OAuth token 刷新
- `mark_credential_dead / healthy` — 管理员覆写
- `list_health` — 健康快照
- `oauth_start / oauth_finish` — OAuth 流程编排
- `get_dispatch_table` — 路由解析

**关键设计决策：** 所有运行时读写走内存（ArcSwap + DashMap），无 DB 依赖。状态通过 snapshot/restore 机制实现持久化——但这个持久化逻辑在 App 层，不在 SDK。

### 3.4 retry — 凭证轮转重试

**核心函数：** `retry_with_credentials<C, F>()` / `retry_with_credentials_stream<C, F>()`

**重试流程：**
1. 通过 `health.is_available(model)` 过滤可用凭证
2. Round-robin 或 affinity 选择候选
3. 发送请求 → 分类响应：
   - **Success (2xx)** → 记录成功，绑定 affinity，返回
   - **AuthDead (401/403)** → 尝试 `refresh_credential()`，刷新成功则重试一次，否则标记 dead
   - **RateLimited (429)** → 有 retry-after 则记录 cooldown 跳下一凭证；无则指数退避重试
   - **TransientError / PermanentError** → 跳下一凭证
4. 所有凭证耗尽 → `AllCredentialsExhausted`

### 3.5 health — 凭证健康状态

**`CredentialHealth` trait：**
```rust
fn is_available(&self, model: Option<&str>) -> bool;
fn record_error(&mut self, status: u16, model: Option<&str>, retry_after_ms: Option<u64>);
fn record_success(&mut self, model: Option<&str>);
```

**实现：**
- `SimpleHealth` — 仅 dead boolean（401/403）
- `ModelCooldownHealth` — 按模型粒度的 cooldown + 全局 cooldown + 指数退避（1s → 60s max）

### 3.6 channel — 渠道抽象

**`Channel` trait：**
```rust
pub trait Channel: Send + Sync + 'static {
    const ID: &'static str;
    type Settings: ChannelSettings;
    type Credential: ChannelCredential;
    type Health: CredentialHealth;

    fn dispatch_table(&self) -> DispatchTable;
    fn prepare_request(...) -> Result<http::Request<Vec<u8>>>;
    fn classify_response(status, headers, body) -> ResponseClassification;
    fn refresh_credential(...) -> impl Future + Send;
    // ... 更多可选方法
}
```

**注册方式：** `inventory::submit!` — 新渠道仅需实现 trait 并注册，不改其他文件。

### 3.7 channels — 14 个内置渠道

| 渠道 ID | 说明 |
|---------|------|
| `openai` | OpenAI API |
| `anthropic` | Anthropic Claude API |
| `claudecode` | Claude Code（session cookie 鉴权） |
| `codex` | Codex |
| `vertex` | Google Vertex AI |
| `vertexexpress` | Vertex Express |
| `aistudio` | Google AI Studio |
| `geminicli` | Gemini CLI |
| `antigravity` | Antigravity |
| `nvidia` | NVIDIA |
| `deepseek` | DeepSeek |
| `groq` | Groq |
| `openrouter` | OpenRouter |
| `custom` | 自定义 base URL |

每个渠道定义：Settings（base_url、retry config）、Credential（API key / OAuth / cookie）、Health、dispatch_table()、prepare_request()、classify_response()、refresh_credential()。

### 3.8 dispatch — 路由解析

```rust
pub enum RouteImplementation {
    Passthrough,                        // 原样转发
    TransformTo { destination: RouteKey }, // 跨协议转换
    Local,                              // 本地处理（不调上游）
    Unsupported,                        // 返回 501
}
```

每个渠道的 `dispatch_table()` 定义 `(OperationFamily, ProtocolKind) → RouteImplementation` 映射。例：OpenAI 渠道收到 Claude 协议请求 → `TransformTo(OpenAi)`。

### 3.9 affinity — Prompt-Cache 亲和

`CacheAffinityPool` — LRU 缓存（默认 4096 key），将 prompt-cache key 绑定到特定凭证索引。

- Claude：解析 `cache_control` 块，SHA256 哈希内容
- OpenAI：检测 `cache_control` 存在
- Gemini：检测 `cached_content`

TTL：Claude ephemeral 1h，OpenAI 24h，Gemini 1h，默认 5m。

### 3.10 billing — 定价与计费

```rust
pub fn estimate_billing(
    model_prices: &[ModelPrice],
    context: &BillingContext,
    usage: &Usage,
) -> Option<BillingResult>;
```

- 检测 billing mode（Default / Flex / Scale / Priority）
- 选择定价层级
- 按 input / output / cache / tools 分项计算
- 内置各渠道定价（JSON 格式，`OnceLock` 启动加载）

### 3.11 utils — 辅助工具

- `oauth.rs` — PKCE / state 生成、callback URL 解析
- `oauth2_refresh.rs` — OAuth2 token 刷新
- `claude_cache_control.rs` / `vertex_normalize.rs` / `code_assist_envelope.rs` / `google_quota.rs` / `claudecode_cookie.rs` — 渠道特定的请求/响应变换

---

## SDK 职责边界判定

### 纯库能力（无状态 / 无 I/O）

- 全部 `gproxy-protocol`
- `dispatch.rs` — 不可变路由表
- `billing.rs` — 纯计算
- `usage.rs` — 纯提取
- `response.rs` — 纯数据类型
- `request.rs` — 纯数据类型
- `transform_dispatch.rs` — 纯转换

### 运行时状态管理（SDK 内部）

- `store.rs` — `ProviderStore` 是 SDK 内部的运行时主状态持有者：管理所有 provider 的凭证列表、健康状态、亲和绑定
- `health.rs` — `ModelCooldownHealth` 在 store 内维护每凭证的 cooldown / dead 状态
- `affinity.rs` — `CacheAffinityPool` 在 store 内维护 prompt-cache 绑定
- `engine.rs` — `GproxyEngine` 持有 `Arc<ProviderStore>` 和 HTTP client

### 它是 "provider execution SDK" 还是 "运行时主状态管理"？

**答：两者兼有。** `gproxy-provider` 不仅是执行层，它通过 `ProviderStore` 持有并管理了：

1. **凭证列表与排序** — 运行时谁是活凭证、谁被 dead
2. **健康状态** — 429 cooldown、401 dead、指数退避
3. **亲和绑定** — 哪个 prompt-cache key 绑到哪个凭证
4. **凭证热更新** — `add/update/remove_credential`、`apply_credential_update`（OAuth 刷新）
5. **设置热更新** — `update_provider_settings`

这些构成了系统的**运行时主状态**。App 层的 `AppState` 对 provider/credential 的管理最终都要同步到 `ProviderStore`。

**但 SDK 不承担持久化。** DB 读写、启动恢复、定期快照全部在 App 层。SDK 提供 snapshot API（`ProviderSnapshot`、`CredentialSnapshot`、`CredentialHealthSnapshot`）供 App 层序列化/反序列化。

---

## 与 App 层的耦合点

SDK 自身不依赖 App 层，但 App 层在以下位置深度依赖 SDK：

| App 层位置 | 依赖的 SDK 能力 | 耦合性质 |
|-----------|----------------|---------|
| `AppState.engine` | `ArcSwap<GproxyEngine>` | 持有 SDK 引擎实例 |
| `bootstrap.rs` | `GproxyEngine::builder()` | 启动时构建引擎 |
| `admin/providers.rs` | `ProviderStore` CRUD | admin mutation 同步到 SDK store |
| `admin/credentials.rs` | `ProviderStore` 凭证操作 | 凭证 CRUD 同步 |
| `provider/handler.rs` | `engine.execute_request()` | 请求代理核心路径 |
| `provider/websocket.rs` | `engine.execute_ws()` | WebSocket 代理 |
| `provider/oauth.rs` | `engine.oauth_start/finish()` | OAuth 流程 |
| `middleware/classify.rs` | `OperationFamily` / `ProtocolKind` | 请求分类 |
| `middleware/model_alias.rs` | — | 使用 AppState 内存，不直接用 SDK |
| `middleware/permission.rs` | — | 使用 AppState 内存，不直接用 SDK |

**关键观察：** App 层对 provider/credential 的 admin CRUD 必须执行两步——先写 DB，再同步到 SDK 的 `ProviderStore`。这个双写是当前架构的核心耦合点。
