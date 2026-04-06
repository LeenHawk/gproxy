# gproxy SDK

`gproxy-sdk` 是 gproxy 的 Rust SDK 入口 crate。它把协议类型、路由逻辑和 provider 引擎三层能力统一暴露给上层应用，适合需要自己组装 LLM 代理、网关、转发层或多上游聚合服务的 Rust 开发者。

## 入口结构

`sdk/gproxy-sdk/src/lib.rs` 当前只做了三组 re-export：

- `pub use gproxy_protocol as protocol;`
- `pub use gproxy_provider as provider;`
- `pub use gproxy_routing as routing;`

## 三个 crate 的职责

| crate | 在 `gproxy-sdk` 中的入口 | 职责 |
| --- | --- | --- |
| `gproxy-protocol` | `gproxy_sdk::protocol` | 提供 Claude / OpenAI / Gemini 的 wire-format 类型，以及跨协议 `transform` 转换。 |
| `gproxy-routing` | `gproxy_sdk::routing` | 提供与框架无关的路由分类、模型提取、provider 前缀处理、权限匹配和限流规则匹配等纯逻辑 helper。 |
| `gproxy-provider` | `gproxy_sdk::provider` | 提供基于 `Channel` trait 的多渠道 provider 引擎，包括 `ProviderStore`、`GproxyEngine`、重试、健康状态与后端抽象。 |

## 快速开始

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

## Feature Flags

`sdk/gproxy-sdk/Cargo.toml` 中声明的 feature 如下：

| feature | Cargo 声明 | 说明 |
| --- | --- | --- |
| `default` | `["all-channels"]` | 默认启用全部渠道 feature。 |
| `all-channels` | `["gproxy-provider/all-channels"]` | 转发到 `gproxy-provider/all-channels`。 |
| `openai` | `["gproxy-provider/openai"]` | OpenAI 渠道 feature。 |
| `anthropic` | `["gproxy-provider/anthropic"]` | Anthropic 渠道 feature。 |
| `aistudio` | `["gproxy-provider/aistudio"]` | AI Studio 渠道 feature。 |
| `vertexexpress` | `["gproxy-provider/vertexexpress"]` | Vertex Express 渠道 feature。 |
| `vertex` | `["gproxy-provider/vertex"]` | Vertex 渠道 feature。 |
| `geminicli` | `["gproxy-provider/geminicli"]` | Gemini CLI 渠道 feature。 |
| `claudecode` | `["gproxy-provider/claudecode"]` | Claude Code 渠道 feature。 |
| `codex` | `["gproxy-provider/codex"]` | Codex 渠道 feature。 |
| `antigravity` | `["gproxy-provider/antigravity"]` | Antigravity 渠道 feature。 |
| `nvidia` | `["gproxy-provider/nvidia"]` | NVIDIA 渠道 feature。 |
| `deepseek` | `["gproxy-provider/deepseek"]` | DeepSeek 渠道 feature。 |
| `groq` | `["gproxy-provider/groq"]` | Groq 渠道 feature。 |
| `openrouter` | `["gproxy-provider/openrouter"]` | OpenRouter 渠道 feature。 |
| `custom` | `["gproxy-provider/custom"]` | 自定义兼容渠道 feature。 |
| `redis` | 未在 `sdk/gproxy-sdk/Cargo.toml` 或 `sdk/gproxy-provider/Cargo.toml` 的 `[features]` 中声明 | 当前 SDK 层没有 `redis` feature flag。workspace 顶层存在 `redis` 依赖，但它不是这里的 feature。 |

## 说明

`gproxy-provider/Cargo.toml` 也声明了同名的单渠道 features 和 `all-channels`，但当前源码中没有检索到 `#[cfg(feature = "...")]` 条件编译入口。因此这里按 Cargo 声明说明 feature 名称，不把它描述成已经生效的渠道裁剪机制。
