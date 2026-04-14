# gproxy-engine

[![crates.io](https://img.shields.io/crates/v/gproxy-engine.svg)](https://crates.io/crates/gproxy-engine)
[![docs.rs](https://docs.rs/gproxy-engine/badge.svg)](https://docs.rs/gproxy-engine)
[![license](https://img.shields.io/crates/l/gproxy-engine.svg)](https://github.com/LeenHawk/gproxy)

Multi-channel LLM orchestration engine in Rust. Drops a ready-to-use
`GproxyEngine` with:

- **Provider store** — many named providers, each backed by one channel
  and many credentials, hot-swappable at runtime.
- **Retry loop** — per-credential 429 retries with retry-after handling,
  token-refresh on `401`/`403`, automatic credential rotation on
  persistent failures.
- **Credential affinity** — optional prompt-cache hint affinity that
  pins requests with similar prefixes to the same credential to
  maximise upstream prompt-cache hits.
- **Dispatch consumer** — walks each channel's `DispatchTable` and
  drives cross-protocol transforms (via
  [`gproxy-protocol`]'s runtime dispatcher) when a client speaks one
  protocol and the upstream speaks another.
- **Pluggable backends** — `RateLimitBackend`, `QuotaBackend`, and
  `AffinityBackend` traits (with in-memory defaults) so you can wire in
  Redis / a database for multi-node deployments.
- **Routing helpers** — `classify`, `permission`, `rate_limit`,
  `provider_prefix`, `model_alias`, `model_extraction`, header
  sanitization — framework-independent so your HTTP middleware can
  share the same parsing as the engine.

> **L2 layer** of the [gproxy SDK](https://github.com/LeenHawk/gproxy).
> Use this crate when you're building a multi-provider LLM gateway,
> router, or aggregator in Rust.

| Crate | Layer | What it covers |
|---|---|---|
| [`gproxy-protocol`] | L0 | Wire types + cross-protocol transforms |
| [`gproxy-channel`]  | L1 | `Channel` trait, 14 channels, `execute_once` |
| `gproxy-engine` (this crate) | L2 | Multi-channel engine + store + retry + affinity + backends + routing |
| [`gproxy-sdk`] | facade | Re-exports the three layers under canonical names |

[`gproxy-protocol`]: https://crates.io/crates/gproxy-protocol
[`gproxy-channel`]:  https://crates.io/crates/gproxy-channel
[`gproxy-sdk`]:      https://crates.io/crates/gproxy-sdk

## Quick start

```rust
use gproxy_channel::channels::openai::{OpenAiChannel, OpenAiCredential, OpenAiSettings};
use gproxy_channel::health::ModelCooldownHealth;
use gproxy_engine::GproxyEngine;

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
        .enable_usage(true)
        .enable_upstream_log(true)
        .build();

    let providers = engine.store().list_providers().unwrap();
    assert_eq!(providers.len(), 1);
}
```

## Public surface

Top-level re-exports for the most-used types:

```rust
// Multi-channel engine entry points
pub use gproxy_engine::{
    GproxyEngine, ExecuteRequest, ExecuteResult, ExecuteError, ExecuteBody,
    ProviderConfig, built_in_model_prices,
};

// Provider store + credential bookkeeping
pub use gproxy_engine::{
    ProviderStore, ProviderStoreBuilder, ProviderSnapshot, ProviderMutator,
    ProviderRegistry, CredentialSnapshot, CredentialUpdate, CredentialHealthSnapshot,
    EngineEvent, EngineEventSource, OAuthFinishResult,
};

// Backend traits + in-memory implementations
pub use gproxy_engine::{
    RateLimitBackend, QuotaBackend, AffinityBackend,
    InMemoryRateLimit, InMemoryQuota, InMemoryAffinity,
    QuotaBalance, RateLimitWindow, BackendError, QuotaError, QuotaExhausted,
    RateLimitExceeded,
};
```

Advanced users reach the submodules directly:

```rust
use gproxy_engine::routing;   // classify, permission, rate_limit, provider_prefix, ...
use gproxy_engine::backend;   // traits + in-memory impls (same as above re-exports)
```

## Features

All per-channel features forward to [`gproxy-channel`], so
`cargo add gproxy-engine --no-default-features --features openai` will
compile the engine + only the OpenAI channel into the binary.

| Feature | Default | Forwards to |
|---|---|---|
| `all-channels` | ✅ | `gproxy-channel/all-channels` |
| `openai` | via `all-channels` | `gproxy-channel/openai` |
| `anthropic` | via `all-channels` | `gproxy-channel/anthropic` |
| ... one entry per channel ... |  |  |

## License

AGPL-3.0-or-later. See the
[workspace repository](https://github.com/LeenHawk/gproxy) for contribution
and licensing details.
