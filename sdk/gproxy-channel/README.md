# gproxy-channel

[![crates.io](https://img.shields.io/crates/v/gproxy-channel.svg)](https://crates.io/crates/gproxy-channel)
[![docs.rs](https://docs.rs/gproxy-channel/badge.svg)](https://docs.rs/gproxy-channel)
[![license](https://img.shields.io/crates/l/gproxy-channel.svg)](https://github.com/LeenHawk/gproxy)

Single-channel LLM client layer for Rust. Provides the `Channel` trait, 14
pre-built channel implementations (OpenAI, Anthropic, Gemini, Vertex, and
friends), strongly typed credential / request / response types, credential
health tracking, a dispatch table system, and an `execute_once` single-request
pipeline that handles finalize → sanitize → rewrite → prepare_request → HTTP
send → normalize_response → classify_response in one call.

> **L1 layer** of the [gproxy SDK](https://github.com/LeenHawk/gproxy). Use
> this crate when you want a typed client for one provider at a time and
> handle credential rotation / retry yourself; use [`gproxy-engine`] when
> you want automatic multi-channel failover; use [`gproxy-sdk`] for
> everything wired up.

| Crate | Layer | What it covers |
|---|---|---|
| [`gproxy-protocol`] | L0 | Wire types + cross-protocol transforms |
| `gproxy-channel` (this crate) | L1 | `Channel` trait + 14 channels + `execute_once` |
| [`gproxy-engine`] | L2 | Multi-channel `GproxyEngine`, provider store, retry, affinity |
| [`gproxy-sdk`] | facade | Re-exports the three layers under canonical names |

[`gproxy-protocol`]: https://crates.io/crates/gproxy-protocol
[`gproxy-engine`]:   https://crates.io/crates/gproxy-engine
[`gproxy-sdk`]:      https://crates.io/crates/gproxy-sdk

## Supported channels

All gated behind per-channel Cargo features. The `default` feature is
`all-channels`; pick individual features to prune unused channels from the
compiled binary.

`openai`, `anthropic`, `aistudio`, `vertex`, `vertexexpress`, `geminicli`,
`claudecode`, `codex`, `antigravity`, `nvidia`, `deepseek`, `groq`,
`openrouter`, `custom`

Example — only the OpenAI channel compiles in, nothing else:

```bash
cargo add gproxy-channel --no-default-features --features openai
```

## Quick start

```rust
use gproxy_channel::channels::openai::{OpenAiChannel, OpenAiCredential, OpenAiSettings};
use gproxy_channel::dispatch::RouteKey;
use gproxy_channel::executor::execute_once;
use gproxy_channel::request::PreparedRequest;
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let channel = OpenAiChannel;
    let settings = OpenAiSettings::default();
    let credential = OpenAiCredential {
        api_key: std::env::var("OPENAI_API_KEY")?,
    };
    let http_client = wreq::Client::new();

    let request = PreparedRequest {
        method: http::Method::POST,
        route: RouteKey::new(
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ),
        model: Some("gpt-4o-mini".to_string()),
        body: br#"{
            "model": "gpt-4o-mini",
            "messages": [{"role":"user","content":"hi"}]
        }"#.to_vec(),
        headers: http::HeaderMap::new(),
    };

    let outcome = execute_once(&channel, &credential, &settings, &http_client, request).await?;
    println!("status = {}", outcome.response.status);
    println!("classification = {:?}", outcome.classification);
    Ok(())
}
```

See [`examples/hello_openai.rs`](examples/hello_openai.rs) for a runnable
version you can drive against real OpenAI.

## What's in the box

- **`Channel` trait** — implement once per upstream provider. Declares the
  channel's dispatch table, HTTP request construction, response
  classification, OAuth flow (if any), and optional local routes.
- **14 built-in channels** — OpenAI (`/v1/chat/completions` and
  `/v1/responses`), Anthropic Claude, Google AI Studio (Gemini), Vertex
  (service-account JWT + Vertex Express API-key), Gemini CLI (OAuth),
  Claude Code (session cookie), Codex, Antigravity, NVIDIA, DeepSeek,
  Groq, OpenRouter, and a generic `custom` channel.
- **Credential types** — `ChannelCredential` trait + per-channel concrete
  types (API keys, OAuth token bundles, cookie sessions, GCP service
  accounts).
- **Credential health** — `CredentialHealth` trait with a
  `ModelCooldownHealth` default implementation that tracks per-model
  cooldown windows and 429 / 5xx penalties.
- **Dispatch table** — each channel declares a `DispatchTable` that maps
  `(OperationFamily, ProtocolKind)` pairs to `Passthrough`,
  `TransformTo { destination }`, `Local`, or `Unsupported` routes. This
  is how the engine decides whether to run a cross-protocol transform
  before sending.
- **`execute_once` executor** — the single-request pipeline for users who
  don't need a multi-channel engine.
- **`apply_outgoing_rules`** — the single in-tree invocation point for
  `gproxy-channel`'s sanitize / rewrite rule application.
- **Common channel settings** — every `ChannelSettings` shape embeds a
  `CommonChannelSettings` block (`#[serde(flatten)]`) holding
  `user_agent`, `max_retries_on_429`, `sanitize_rules`, and
  `rewrite_rules`, so channel authors only write per-channel fields.

## Features

| Feature | Default | Effect |
|---|---|---|
| `all-channels` | ✅ | Equivalent to enabling every per-channel feature |
| `openai` | via `all-channels` | Compiles the `channels::openai` module |
| `anthropic` | via `all-channels` | Compiles the `channels::anthropic` module |
| `aistudio` | via `all-channels` | AI Studio (Gemini REST) |
| `vertex` | via `all-channels` | Vertex AI (service-account JWT) |
| `vertexexpress` | via `all-channels` | Vertex AI Express (API key) |
| `geminicli` | via `all-channels` | Gemini CLI (OAuth) |
| `claudecode` | via `all-channels` | Anthropic Claude Code (session cookie) |
| `codex` | via `all-channels` | OpenAI Codex |
| `antigravity` | via `all-channels` | Antigravity |
| `nvidia` | via `all-channels` | NVIDIA NIM |
| `deepseek` | via `all-channels` | DeepSeek |
| `groq` | via `all-channels` | Groq |
| `openrouter` | via `all-channels` | OpenRouter |
| `custom` | via `all-channels` | Generic OpenAI-compatible channel |

## License

AGPL-3.0-or-later. See the
[workspace repository](https://github.com/LeenHawk/gproxy) for contribution
and licensing details.
