# gproxy SDK / gproxy SDK

[中文](#中文) | [English](#english)

---

## 中文

`gproxy-sdk` 是 gproxy 的 Rust SDK 入口 crate。它把协议类型、路由逻辑和 provider 引擎三层能力统一暴露给上层应用，适合需要自行组装 LLM 代理、网关、转发层或多上游聚合服务的 Rust 开发者。

### 入口结构

`sdk/gproxy-sdk/src/lib.rs` 当前只做了三组 re-export：

- `pub use gproxy_protocol as protocol;`
- `pub use gproxy_provider as provider;`
- `pub use gproxy_routing as routing;`

### 三个 crate 的职责

下表是中英共享表格，列出 `gproxy-sdk` 暴露的三个核心 crate。

| crate | 在 `gproxy-sdk` 中的入口 / Entry in `gproxy-sdk` | 职责 / Responsibility |
| --- | --- | --- |
| `gproxy-protocol` | `gproxy_sdk::protocol` | 提供 Claude / OpenAI / Gemini 的 wire-format 类型，以及跨协议 `transform` 转换。 / Provides wire-format types for Claude, OpenAI, and Gemini, plus cross-protocol `transform` conversions. |
| `gproxy-routing` | `gproxy_sdk::routing` | 提供与框架无关的路由分类、模型提取、provider 前缀处理、权限匹配和限流规则匹配等纯逻辑 helper。 / Provides framework-agnostic helpers for route classification, model extraction, provider-prefix handling, permission matching, and rate-limit rule matching. |
| `gproxy-provider` | `gproxy_sdk::provider` | 提供基于 `Channel` trait 的多渠道 provider 引擎，包括 `ProviderStore`、`GproxyEngine`、重试、健康状态与后端抽象。 / Provides the multi-channel provider engine built around the `Channel` trait, including `ProviderStore`, `GproxyEngine`, retries, health state, and backend abstractions. |

### 快速开始

先添加 SDK。若只需要 OpenAI 渠道，可以执行 `cargo add gproxy-sdk --no-default-features --features openai`。

```rust
use gproxy_sdk::provider::{
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

let providers = engine.store().list_providers().unwrap();
assert_eq!(providers.len(), 1);
```

上面的共享 `rust` 示例展示了如何构建一个仅包含单个 OpenAI provider 的最小 `GproxyEngine`。

### Feature Flags

`sdk/gproxy-sdk/Cargo.toml` 中声明的 feature 如下，表格为中英共享内容：

| feature | Cargo 声明 / Cargo Declaration | 说明 / Notes |
| --- | --- | --- |
| `default` | `["all-channels"]` | 默认启用全部渠道 feature。 / Enables all channel features by default. |
| `all-channels` | `["gproxy-provider/all-channels"]` | 转发到 `gproxy-provider/all-channels`。 / Forwards to `gproxy-provider/all-channels`. |
| `openai` | `["gproxy-provider/openai"]` | OpenAI 渠道 feature。 / OpenAI channel feature. |
| `anthropic` | `["gproxy-provider/anthropic"]` | Anthropic 渠道 feature。 / Anthropic channel feature. |
| `aistudio` | `["gproxy-provider/aistudio"]` | AI Studio 渠道 feature。 / AI Studio channel feature. |
| `vertexexpress` | `["gproxy-provider/vertexexpress"]` | Vertex Express 渠道 feature。 / Vertex Express channel feature. |
| `vertex` | `["gproxy-provider/vertex"]` | Vertex 渠道 feature。 / Vertex channel feature. |
| `geminicli` | `["gproxy-provider/geminicli"]` | Gemini CLI 渠道 feature。 / Gemini CLI channel feature. |
| `claudecode` | `["gproxy-provider/claudecode"]` | Claude Code 渠道 feature。 / Claude Code channel feature. |
| `codex` | `["gproxy-provider/codex"]` | Codex 渠道 feature。 / Codex channel feature. |
| `antigravity` | `["gproxy-provider/antigravity"]` | Antigravity 渠道 feature。 / Antigravity channel feature. |
| `nvidia` | `["gproxy-provider/nvidia"]` | NVIDIA 渠道 feature。 / NVIDIA channel feature. |
| `deepseek` | `["gproxy-provider/deepseek"]` | DeepSeek 渠道 feature。 / DeepSeek channel feature. |
| `groq` | `["gproxy-provider/groq"]` | Groq 渠道 feature。 / Groq channel feature. |
| `openrouter` | `["gproxy-provider/openrouter"]` | OpenRouter 渠道 feature。 / OpenRouter channel feature. |
| `custom` | `["gproxy-provider/custom"]` | 自定义兼容渠道 feature。 / Custom compatibility channel feature. |
| `redis` | 未在 `sdk/gproxy-sdk/Cargo.toml` 或 `sdk/gproxy-provider/Cargo.toml` 的 `[features]` 中声明。 / Not declared in `[features]` of either `sdk/gproxy-sdk/Cargo.toml` or `sdk/gproxy-provider/Cargo.toml`. | 当前 SDK 层没有 `redis` feature flag；workspace 顶层存在 `redis` 依赖，但它不是这里的 feature。 / The SDK layer currently has no `redis` feature flag; the workspace root has a `redis` dependency, but it is not a feature here. |

### 说明

`gproxy-provider/Cargo.toml` 也声明了同名的单渠道 features 和 `all-channels`，但当前源码中没有检索到 `#[cfg(feature = "...")]` 条件编译入口。因此这里按 Cargo 声明说明 feature 名称，不把它描述成已经生效的渠道裁剪机制。

---

## English

`gproxy-sdk` is the entry crate for the gproxy Rust SDK. It exposes protocol types, routing logic, and the provider engine through one surface, making it suitable for Rust developers who want to assemble their own LLM agent, gateway, forwarding layer, or multi-upstream aggregation service.

### Entry Structure

`sdk/gproxy-sdk/src/lib.rs` currently only performs three re-exports:

- `pub use gproxy_protocol as protocol;`
- `pub use gproxy_provider as provider;`
- `pub use gproxy_routing as routing;`

### Responsibilities of the Three Crates

See the shared bilingual table above for the exact crate entry points and responsibilities exposed by `gproxy-sdk`.

### Quick Start

Add the SDK first. If you only need the OpenAI channel, run `cargo add gproxy-sdk --no-default-features --features openai`.

The shared `rust` example above shows the minimal `GproxyEngine` setup with a single OpenAI provider.

### Feature Flags

See the shared bilingual table above for the features declared in `sdk/gproxy-sdk/Cargo.toml`.

### Notes

`gproxy-provider/Cargo.toml` also declares the same single-channel features and `all-channels`, but the current source tree does not expose any `#[cfg(feature = "...")]` conditional compilation entry points. Because of that, this document describes the feature names as declared in Cargo rather than as an already-effective channel-pruning mechanism.
