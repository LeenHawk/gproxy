---
title: Rust SDK
description: Using gproxy-sdk to embed the provider engine in your own Rust application.
---

`gproxy-sdk` is the entry crate for the gproxy Rust SDK. It exposes the
protocol types, routing helpers, and the provider engine through one
surface — suitable for Rust developers who want to assemble their own LLM
agent, gateway, forwarding layer, or multi-upstream aggregation service
without running the full gproxy server.

## What's in the umbrella

`sdk/gproxy-sdk/src/lib.rs` does three re-exports:

- `pub use gproxy_protocol as protocol;`
- `pub use gproxy_provider as provider;`
- `pub use gproxy_routing as routing;`

| Crate | Re-exported as | Responsibility |
| --- | --- | --- |
| `gproxy-protocol` | `gproxy_sdk::protocol` | Wire-format types for Claude, OpenAI, and Gemini, plus cross-protocol `transform` conversions. |
| `gproxy-routing` | `gproxy_sdk::routing` | Framework-agnostic helpers for route classification, model extraction, provider-prefix handling, and permission / rate-limit matching. |
| `gproxy-provider` | `gproxy_sdk::provider` | The multi-channel provider engine: the `Channel` trait, `ProviderStore`, `GproxyEngine`, retries, health state, and backend abstractions. |

None of the three has a dependency on the database, the HTTP server, or
Axum. You can build an entirely different service on top of them.

## Quick start

Add the SDK. If you only need one channel, disable defaults and opt into
the feature you want:

```bash
cargo add gproxy-sdk --no-default-features --features openai
```

Then build a minimal engine:

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

This is the minimal viable setup: one provider, one credential, health
tracked by `ModelCooldownHealth`, usage and upstream logging on (body
capture off).

## Feature flags

Declared in `sdk/gproxy-sdk/Cargo.toml`:

| Feature | Forwards to | Notes |
| --- | --- | --- |
| `default` | `all-channels` | Enables every channel. |
| `all-channels` | `gproxy-provider/all-channels` | Umbrella for all channel features. |
| `openai` | `gproxy-provider/openai` | OpenAI channel. |
| `anthropic` | `gproxy-provider/anthropic` | Anthropic channel. |
| `aistudio` | `gproxy-provider/aistudio` | Google AI Studio channel. |
| `vertex` | `gproxy-provider/vertex` | Vertex AI channel. |
| `vertexexpress` | `gproxy-provider/vertexexpress` | Vertex AI Express channel. |
| `geminicli` | `gproxy-provider/geminicli` | Gemini CLI channel. |
| `claudecode` | `gproxy-provider/claudecode` | Claude Code channel. |
| `codex` | `gproxy-provider/codex` | Codex channel. |
| `antigravity` | `gproxy-provider/antigravity` | Antigravity channel. |
| `nvidia` | `gproxy-provider/nvidia` | NVIDIA channel. |
| `deepseek` | `gproxy-provider/deepseek` | DeepSeek channel. |
| `groq` | `gproxy-provider/groq` | Groq channel. |
| `openrouter` | `gproxy-provider/openrouter` | OpenRouter channel. |
| `custom` | `gproxy-provider/custom` | Custom OpenAI-compatible channel. |

The SDK layer does **not** expose a `redis` feature; the workspace uses
Redis only from the full server binary.

## When to use the SDK vs. the binary

- **Use the binary** when you want a working multi-tenant LLM proxy
  with a console, storage, and background workers out of the box.
- **Use the SDK** when you need the routing / protocol-transform /
  provider-engine pieces *inside* a larger Rust service — for example,
  an agent runtime that occasionally needs to fan out to several
  upstreams, or a custom gateway with its own auth and storage model.

Most of the interesting types — `GproxyEngine`, `ProviderStore`, the
`Channel` trait, `ModelCooldownHealth`, `transform::*` — have
doc-comments in their source files under `sdk/`.
