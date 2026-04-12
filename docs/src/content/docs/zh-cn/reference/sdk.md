---
title: Rust SDK
description: 使用 gproxy-sdk 把 provider 引擎嵌入到你自己的 Rust 应用。
---

`gproxy-sdk` 是 GPROXY Rust SDK 的入口 crate，把协议类型、路由工具和供应商引擎
统一暴露出来 —— 方便在不跑完整 GPROXY 服务器的情况下，自己组装 LLM agent、
网关、转发层或多上游聚合服务。

## Umbrella 里有什么

`sdk/gproxy-sdk/src/lib.rs` 做了三处 re-export：

- `pub use gproxy_protocol as protocol;`
- `pub use gproxy_provider as provider;`
- `pub use gproxy_routing as routing;`

| Crate | 在 `gproxy-sdk` 中的入口 | 职责 |
| --- | --- | --- |
| `gproxy-protocol` | `gproxy_sdk::protocol` | Claude / OpenAI / Gemini 的协议类型，以及跨协议 `transform` 转换。 |
| `gproxy-routing` | `gproxy_sdk::routing` | 路由分类、模型抽取、provider-prefix 处理、权限/限流匹配等框架无关工具。 |
| `gproxy-provider` | `gproxy_sdk::provider` | 多通道 provider 引擎：`Channel` trait、`ProviderStore`、`GproxyEngine`、重试、健康状态、后端抽象。 |

这三个 crate 都不依赖数据库、HTTP server 或 Axum —— 可以在上面构建完全不同的服务。

## 快速入门

添加 SDK。只需要某个通道的话，关闭默认 feature，显式启用即可：

```bash
cargo add gproxy-sdk --no-default-features --features openai
```

然后构建一个最小引擎：

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

这是 MVP 配置：一个 provider、一个凭证，用 `ModelCooldownHealth` 跟踪健康状态，
打开 usage 和上游日志 (body 捕获关闭)。

## Feature flag

定义在 `sdk/gproxy-sdk/Cargo.toml`：

| Feature | 转发到 | 说明 |
| --- | --- | --- |
| `default` | `all-channels` | 默认启用所有通道。 |
| `all-channels` | `gproxy-provider/all-channels` | 全部通道的 umbrella。 |
| `openai` | `gproxy-provider/openai` | OpenAI 通道。 |
| `anthropic` | `gproxy-provider/anthropic` | Anthropic 通道。 |
| `aistudio` | `gproxy-provider/aistudio` | Google AI Studio 通道。 |
| `vertex` | `gproxy-provider/vertex` | Vertex AI 通道。 |
| `vertexexpress` | `gproxy-provider/vertexexpress` | Vertex AI Express 通道。 |
| `geminicli` | `gproxy-provider/geminicli` | Gemini CLI 通道。 |
| `claudecode` | `gproxy-provider/claudecode` | Claude Code 通道。 |
| `codex` | `gproxy-provider/codex` | Codex 通道。 |
| `antigravity` | `gproxy-provider/antigravity` | Antigravity 通道。 |
| `nvidia` | `gproxy-provider/nvidia` | NVIDIA 通道。 |
| `deepseek` | `gproxy-provider/deepseek` | DeepSeek 通道。 |
| `groq` | `gproxy-provider/groq` | Groq 通道。 |
| `openrouter` | `gproxy-provider/openrouter` | OpenRouter 通道。 |
| `custom` | `gproxy-provider/custom` | 自定义 OpenAI 兼容通道。 |

SDK 层**没有** `redis` feature；只有完整 server 会用到 Redis。

## 选 SDK 还是选二进制?

- **选二进制**：想要开箱即用的多租户 LLM 代理，带控制台、存储和后台 worker。
- **选 SDK**：想把路由 / 协议转换 / 供应商引擎等组件嵌入到更大的 Rust 服务里。
  比如一个 agent runtime 偶尔需要扇出到若干上游，或是一个自定义网关
  (自带鉴权与存储模型)。

大多数重要类型 —— `GproxyEngine`、`ProviderStore`、`Channel` trait、
`ModelCooldownHealth`、`transform::*` —— 都在 `sdk/` 下的源码里有 doc-comment。
