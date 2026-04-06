# gproxy SDK

`gproxy-sdk` is the entry crate for the gproxy Rust SDK. It exposes protocol types, routing logic, and the provider engine through one surface, making it suitable for Rust developers who want to assemble their own LLM agent, gateway, forwarding layer, or multi-upstream aggregation service.

## Entry Structure

`sdk/gproxy-sdk/src/lib.rs` currently only performs three re-exports:

- `pub use gproxy_protocol as protocol;`
- `pub use gproxy_provider as provider;`
- `pub use gproxy_routing as routing;`

## Responsibilities of the Three Crates

The table below lists the three core crates exposed by `gproxy-sdk`.

| crate | Entry in `gproxy-sdk` | Responsibility |
| --- | --- | --- |
| `gproxy-protocol` | `gproxy_sdk::protocol` | Provides wire-format types for Claude, OpenAI, and Gemini, plus cross-protocol `transform` conversions. |
| `gproxy-routing` | `gproxy_sdk::routing` | Provides framework-agnostic helpers for route classification, model extraction, provider-prefix handling, permission matching, and rate-limit rule matching. |
| `gproxy-provider` | `gproxy_sdk::provider` | Provides the multi-channel provider engine built around the `Channel` trait, including `ProviderStore`, `GproxyEngine`, retries, health state, and backend abstractions. |

## Quick Start

Add the SDK first. If you only need the OpenAI channel, run `cargo add gproxy-sdk --no-default-features --features openai`.

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

The example above shows the minimal `GproxyEngine` setup with a single OpenAI provider.

## Feature Flags

Features declared in `sdk/gproxy-sdk/Cargo.toml`:

| feature | Cargo Declaration | Notes |
| --- | --- | --- |
| `default` | `["all-channels"]` | Enables all channel features by default. |
| `all-channels` | `["gproxy-provider/all-channels"]` | Forwards to `gproxy-provider/all-channels`. |
| `openai` | `["gproxy-provider/openai"]` | OpenAI channel feature. |
| `anthropic` | `["gproxy-provider/anthropic"]` | Anthropic channel feature. |
| `aistudio` | `["gproxy-provider/aistudio"]` | AI Studio channel feature. |
| `vertexexpress` | `["gproxy-provider/vertexexpress"]` | Vertex Express channel feature. |
| `vertex` | `["gproxy-provider/vertex"]` | Vertex channel feature. |
| `geminicli` | `["gproxy-provider/geminicli"]` | Gemini CLI channel feature. |
| `claudecode` | `["gproxy-provider/claudecode"]` | Claude Code channel feature. |
| `codex` | `["gproxy-provider/codex"]` | Codex channel feature. |
| `antigravity` | `["gproxy-provider/antigravity"]` | Antigravity channel feature. |
| `nvidia` | `["gproxy-provider/nvidia"]` | NVIDIA channel feature. |
| `deepseek` | `["gproxy-provider/deepseek"]` | DeepSeek channel feature. |
| `groq` | `["gproxy-provider/groq"]` | Groq channel feature. |
| `openrouter` | `["gproxy-provider/openrouter"]` | OpenRouter channel feature. |
| `custom` | `["gproxy-provider/custom"]` | Custom compatibility channel feature. |
| `redis` | Not declared in `[features]` of either `sdk/gproxy-sdk/Cargo.toml` or `sdk/gproxy-provider/Cargo.toml`. | The SDK layer currently has no `redis` feature flag; the workspace root has a `redis` dependency, but it is not a feature here. |

## Notes

`gproxy-provider/Cargo.toml` also declares the same single-channel features and `all-channels`, but the current source tree does not expose any `#[cfg(feature = "...")]` conditional compilation entry points. Because of that, this document describes the feature names as declared in Cargo rather than as an already-effective channel-pruning mechanism.
