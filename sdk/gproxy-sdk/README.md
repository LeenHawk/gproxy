# gproxy-sdk

[![crates.io](https://img.shields.io/crates/v/gproxy-sdk.svg)](https://crates.io/crates/gproxy-sdk)
[![docs.rs](https://docs.rs/gproxy-sdk/badge.svg)](https://docs.rs/gproxy-sdk)
[![license](https://img.shields.io/crates/l/gproxy-sdk.svg)](https://github.com/LeenHawk/gproxy)

Layered SDK for building multi-channel LLM proxy / gateway applications in
Rust. `gproxy-sdk` is the **facade** crate — it re-exports three underlying
layers under canonical module names so you can pick the granularity that
matches your use case.

```text
gproxy_sdk
├── protocol  →  gproxy-protocol  (L0: wire types + transforms)
├── channel   →  gproxy-channel   (L1: Channel trait + concrete channels + execute_once)
└── engine    →  gproxy-engine    (L2: GproxyEngine + store + retry + affinity + routing)
```

## Which layer should I pick?

| Your need | Import | Dep weight |
|---|---|---|
| Just wire types and cross-protocol transforms | [`gproxy-protocol`] directly | light (`serde`, `http`, `regex`) |
| A typed single-provider client with credential + retry | [`gproxy-channel`] directly | medium (+ `wreq`, `tokio`, `inventory`) |
| A multi-provider LLM gateway / router | [`gproxy-engine`] directly | heavy (+ `dashmap`, `arc-swap`, backend traits) |
| Give me everything, I don't want to think about layers | `gproxy-sdk` (this crate) | same as `gproxy-engine` |

You can always start with `gproxy-sdk` and later `cargo remove gproxy-sdk`
in favour of the lower layers if you want a slimmer dep tree.

[`gproxy-protocol`]: https://crates.io/crates/gproxy-protocol
[`gproxy-channel`]:  https://crates.io/crates/gproxy-channel
[`gproxy-engine`]:   https://crates.io/crates/gproxy-engine

## Quick start

```rust
use gproxy_sdk::channel::channels::openai::{OpenAiChannel, OpenAiCredential, OpenAiSettings};
use gproxy_sdk::channel::health::ModelCooldownHealth;
use gproxy_sdk::engine::GproxyEngine;

#[tokio::main]
async fn main() {
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
        .build();

    let providers = engine.store().list_providers().unwrap();
    assert_eq!(providers.len(), 1);
}
```

## Feature flags

Every per-channel feature is forwarded to both `gproxy-channel` and
`gproxy-engine` so enabling a feature on `gproxy-sdk` prunes the unused
channels from the compiled binary all the way down.

```bash
cargo add gproxy-sdk --no-default-features --features openai
```

| Feature | Forwards to |
|---|---|
| `all-channels` (default) | `gproxy-channel/all-channels`, `gproxy-engine/all-channels` |
| `openai` | `gproxy-channel/openai`, `gproxy-engine/openai` |
| `anthropic` | `gproxy-channel/anthropic`, `gproxy-engine/anthropic` |
| `aistudio` | `gproxy-channel/aistudio`, `gproxy-engine/aistudio` |
| `vertex` | `gproxy-channel/vertex`, `gproxy-engine/vertex` |
| `vertexexpress` | `gproxy-channel/vertexexpress`, `gproxy-engine/vertexexpress` |
| `geminicli` | `gproxy-channel/geminicli`, `gproxy-engine/geminicli` |
| `claudecode` | `gproxy-channel/claudecode`, `gproxy-engine/claudecode` |
| `codex` | `gproxy-channel/codex`, `gproxy-engine/codex` |
| `antigravity` | `gproxy-channel/antigravity`, `gproxy-engine/antigravity` |
| `nvidia` | `gproxy-channel/nvidia`, `gproxy-engine/nvidia` |
| `deepseek` | `gproxy-channel/deepseek`, `gproxy-engine/deepseek` |
| `groq` | `gproxy-channel/groq`, `gproxy-engine/groq` |
| `openrouter` | `gproxy-channel/openrouter`, `gproxy-engine/openrouter` |
| `custom` | `gproxy-channel/custom`, `gproxy-engine/custom` |

## License

AGPL-3.0-or-later. See the
[workspace repository](https://github.com/LeenHawk/gproxy) for contribution
and licensing details.
