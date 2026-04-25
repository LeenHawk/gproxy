---
title: Rust SDK
description: Using gproxy-sdk to embed the provider engine in your own Rust application.
---

`gproxy-sdk` is the entry crate for the GPROXY Rust SDK. It exposes the
protocol types, routing helpers, and the provider engine through one
surface — suitable for Rust developers who want to assemble their own LLM
agent, gateway, forwarding layer, or multi-upstream aggregation service
without running the full GPROXY server.

## What's in the umbrella

`sdk/gproxy-sdk/src/lib.rs` re-exports the three layers:

- `pub use gproxy_protocol as protocol;`
- `pub use gproxy_channel as channel;`
- `pub use gproxy_engine as engine;`

| Crate | Re-exported as | Layer | Responsibility |
| --- | --- | --- | --- |
| `gproxy-protocol` | `gproxy_sdk::protocol` | L0 | Wire-format types for Claude, OpenAI, and Gemini, plus cross-protocol `transform` conversions. Light dependencies, no HTTP. |
| `gproxy-channel` | `gproxy_sdk::channel` | L1 | The `Channel` trait, concrete channel implementations (OpenAI, Anthropic, Gemini, …), credential types, request / response types, billing, health tracking, and token counting. Use this layer when you want a strongly typed single-provider client. |
| `gproxy-engine` | `gproxy_sdk::engine` | L2 | The full multi-channel `GproxyEngine`, `ProviderStore`, retry / credential affinity, backend traits for rate-limit / quota / affinity state, and routing helpers. Use this layer to build your own LLM gateway. |

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
use gproxy_sdk::channel::{
    channels::openai::{OpenAiChannel, OpenAiCredential, OpenAiSettings},
    health::ModelCooldownHealth,
};
use gproxy_sdk::engine::GproxyEngine;

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
| `all-channels` | `gproxy-channel/all-channels` + `gproxy-engine/all-channels` | Umbrella for all channel features. |
| `openai` | `gproxy-channel/openai` + `gproxy-engine/openai` | OpenAI channel. |
| `anthropic` | `gproxy-channel/anthropic` + `gproxy-engine/anthropic` | Anthropic channel. |
| `aistudio` | `gproxy-channel/aistudio` + `gproxy-engine/aistudio` | Google AI Studio channel. |
| `vertex` | `gproxy-channel/vertex` + `gproxy-engine/vertex` | Vertex AI channel. |
| `vertexexpress` | `gproxy-channel/vertexexpress` + `gproxy-engine/vertexexpress` | Vertex AI Express channel. |
| `geminicli` | `gproxy-channel/geminicli` + `gproxy-engine/geminicli` | Gemini CLI channel. |
| `claudecode` | `gproxy-channel/claudecode` + `gproxy-engine/claudecode` | Claude Code channel. |
| `codex` | `gproxy-channel/codex` + `gproxy-engine/codex` | Codex channel. |
| `antigravity` | `gproxy-channel/antigravity` + `gproxy-engine/antigravity` | Antigravity channel. |
| `nvidia` | `gproxy-channel/nvidia` + `gproxy-engine/nvidia` | NVIDIA channel. |
| `deepseek` | `gproxy-channel/deepseek` + `gproxy-engine/deepseek` | DeepSeek channel. |
| `groq` | `gproxy-channel/groq` + `gproxy-engine/groq` | Groq channel. |
| `openrouter` | `gproxy-channel/openrouter` + `gproxy-engine/openrouter` | OpenRouter channel. |
| `custom` | `gproxy-channel/custom` + `gproxy-engine/custom` | Custom OpenAI-compatible channel. |

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
