---
title: Adding a Channel
description: How to implement a new upstream channel in GPROXY — the Channel trait, routing table, registration, and Cargo feature wiring.
---

A **channel** is the code that speaks a specific upstream's wire
protocol. Adding a new upstream means implementing the `Channel` trait
once, declaring a default routing table, and registering the channel
via `inventory` so both the SDK and the server pick it up
automatically.

This page walks through the minimum steps. Read the existing channels
under [`sdk/gproxy-provider/src/channels/`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-provider/src/channels/)
alongside this page — they are the authoritative reference, and most
new channels start life as a copy of whichever existing one is
closest.

## The moving parts

A channel is made of five things:

1. **A settings struct** (`ChannelSettings`) — `base_url`, timeouts,
   cache rules, rewrite rules, and any channel-specific knobs. Held
   per-provider.
2. **A credential struct** (`ChannelCredential`) — API key, OAuth
   tokens, service account, cookie, etc. One provider can have many
   credentials; the engine rotates through them.
3. **A health type** (`CredentialHealth`) — usually
   `ModelCooldownHealth`, which cools a credential per `(credential,
   model)` on retryable failures.
4. **A `Channel` impl** — the trait with `prepare_request`,
   `classify_response`, an optional `finalize_request` /
   `normalize_response` / `handle_local`, and the default
   `routing_table`.
5. **A registration** — an `inventory::submit!` block that adds a
   `ChannelRegistration` so the channel shows up in the registry at
   startup.

The trait lives in
[`sdk/gproxy-channel/src/channel.rs`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-channel/src/channel.rs).

## Pick a starting point

Before writing anything, copy the existing channel closest to your
target:

| If your upstream is… | Start from |
| --- | --- |
| OpenAI-compatible (self-hosted, third-party gateway) | `openai.rs` or the dedicated `custom.rs` |
| Anthropic-flavored (Messages API, `cache_control`) | `anthropic.rs` |
| Gemini / Vertex | `aistudio.rs`, `vertex.rs`, `vertexexpress.rs` |
| OAuth / cookie-authenticated dev tooling | `claudecode.rs`, `codex.rs` |

Rename the struct, module, and `ID` constant, then work outward from
there.

## 1. Define settings and credentials

```rust
// sdk/gproxy-provider/src/channels/acme.rs
use serde::{Deserialize, Serialize};

use crate::channel::{Channel, ChannelCredential, ChannelSettings};
use crate::routing::{RoutingTable, RouteImplementation, RouteKey};
use crate::health::ModelCooldownHealth;
use crate::registry::ChannelRegistration;
use crate::request::PreparedRequest;
use crate::response::{ResponseClassification, UpstreamError};
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

pub struct AcmeChannel;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AcmeSettings {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries_on_429: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sanitize_rules: Vec<crate::utils::sanitize::SanitizeRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rewrite_rules: Vec<crate::utils::rewrite::RewriteRule>,
}

fn default_base_url() -> String { "https://api.acme.example/v1".into() }

impl ChannelSettings for AcmeSettings {
    fn base_url(&self) -> &str { &self.base_url }
    fn user_agent(&self) -> Option<&str> { self.user_agent.as_deref() }
    fn max_retries_on_429(&self) -> u32 { self.max_retries_on_429.unwrap_or(3) }
    fn sanitize_rules(&self) -> &[crate::utils::sanitize::SanitizeRule] { &self.sanitize_rules }
    fn rewrite_rules(&self) -> &[crate::utils::rewrite::RewriteRule] { &self.rewrite_rules }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AcmeCredential {
    pub api_key: String,
}

impl ChannelCredential for AcmeCredential {}
```

Exposing `sanitize_rules` and `rewrite_rules` on the settings struct
is what plugs your channel into the two cross-cutting features
([Rewrite Rules](/guides/rewrite-rules/) and the sanitizer); it's
three lines and there is almost no reason to skip them.

## 2. Implement `Channel`

The two required methods are `routing_table` and
`prepare_request`. Everything else has a default.

```rust
impl Channel for AcmeChannel {
    const ID: &'static str = "acme";
    type Settings   = AcmeSettings;
    type Credential = AcmeCredential;
    type Health     = ModelCooldownHealth;

    fn routing_table(&self) -> RoutingTable {
        let mut table = RoutingTable::new();
        let pass = |op, proto| (
            RouteKey::new(op, proto),
            RouteImplementation::Passthrough,
        );

        // Acme speaks the OpenAI Chat Completions dialect natively.
        for (key, imp) in [
            pass(OperationFamily::ModelList,              ProtocolKind::OpenAi),
            pass(OperationFamily::GenerateContent,        ProtocolKind::OpenAiChatCompletion),
            pass(OperationFamily::StreamGenerateContent,  ProtocolKind::OpenAiChatCompletion),
        ] {
            table.set(key, imp);
        }
        table
    }

    fn prepare_request(
        &self,
        credential: &Self::Credential,
        settings:   &Self::Settings,
        request:    &PreparedRequest,
    ) -> Result<http::Request<Vec<u8>>, UpstreamError> {
        let url = format!("{}{}", settings.base_url, request.upstream_path);
        let mut builder = http::Request::builder()
            .method(request.method.clone())
            .uri(url)
            .header("Authorization", format!("Bearer {}", credential.api_key));

        for (k, v) in request.headers.iter() {
            builder = builder.header(k, v);
        }
        builder
            .body(request.body.clone())
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))
    }

    fn classify_response(
        &self,
        status: u16,
        _headers: &http::HeaderMap,
        _body:    &[u8],
    ) -> ResponseClassification {
        match status {
            429 | 500..=599 => ResponseClassification::RetryableWithCooldown,
            401 | 403       => ResponseClassification::DisableCredential,
            _               => ResponseClassification::Pass,
        }
    }
}
```

Optional hooks you'll probably want eventually:

- **`finalize_request`** — this is where Anthropic/ClaudeCode apply
  [cache breakpoints](/guides/claude-caching/) and magic-string
  triggers. If your upstream needs body normalization that should be
  visible to routing or cache-affinity logic, put it here — not in
  `prepare_request`.
- **`normalize_response`** — fix up non-standard response fields
  before usage extraction / protocol transform.
- **`handle_local`** — implement `Local` routes in the routing
  table. Most channels only use it for `model_list` / `model_get`
  when paired with an `*-only` routing preset.
- **`model_pricing`** — return a `&'static [ModelPrice]` so GPROXY
  can bill calls without the admin entering prices by hand. See
  `channels/pricing/` for the JSON-backed pattern used by existing
  channels.

## 3. Register the channel

Registration is a single `inventory::submit!` call at the bottom of
the file. It has to sit outside the `impl` block and is what makes
the registry discover the channel at startup:

```rust
fn acme_routing_table() -> RoutingTable {
    AcmeChannel.routing_table()
}

inventory::submit! {
    ChannelRegistration::new(AcmeChannel::ID, acme_routing_table)
}
```

The `ChannelRegistry::collect()` function iterates `inventory` at
startup and indexes every registered channel by its `ID`. No manual
`match` table to update.

## 4. Wire the module into the crate

Add the new module to
[`sdk/gproxy-provider/src/channels/mod.rs`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-provider/src/channels/mod.rs):

```rust
pub mod acme;
```

And, if you want a Cargo feature flag for it (so users can strip
unused channels out of their binary), add it to
[`sdk/gproxy-provider/Cargo.toml`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-provider/Cargo.toml)
and re-export it from
[`sdk/gproxy-sdk/Cargo.toml`](https://github.com/LeenHawk/gproxy/blob/main/sdk/gproxy-sdk/Cargo.toml):

```toml
# gproxy-provider
[features]
acme = []
all-channels = ["openai", "anthropic", /* … */, "acme"]
```

The existing feature flags declare the channel names but don't
currently wrap any `#[cfg(feature = "...")]` blocks around the code —
all channels are compiled in. If you want your channel to actually
drop out when the feature is off, wrap the module import and the
`inventory::submit!` call in a `#[cfg(feature = "acme")]`.

## 5. (Optional) Add a TypeScript UI editor

The embedded console renders channel-aware structured editors for
`settings` and `credentials`. If you want your new channel to get one
instead of the generic JSON textarea, add a schema definition under
`frontend/console/src/modules/providers/channels/` matching the shape
of your settings struct. Existing channels there are the template —
the pattern is a TS type + a form definition.

## Testing

A few checks to run before opening a PR:

- **`cargo test -p gproxy-provider`** — the channel tests live next
  to the channel implementation.
- **`cargo run -p gproxy`** with a seed TOML that has
  `channel = "acme"` and an `AcmeCredential` in `credentials` —
  verifies the registry, settings deserialization, and routing
  table all line up.
- **Hit the actual upstream** with `curl` through the scoped path
  form (`/acme-test/v1/chat/completions`) — the scoped form is the
  cleanest way to isolate the new channel during bring-up. See
  [First Request](/getting-started/first-request/).

## Checklist

- [ ] `ChannelSettings` / `ChannelCredential` structs with `serde`
  defaults.
- [ ] `Channel` impl with `routing_table`, `prepare_request`,
  `classify_response`.
- [ ] (If needed) `finalize_request`, `normalize_response`,
  `handle_local`.
- [ ] `inventory::submit!` registration at the bottom of the file.
- [ ] `pub mod your_channel;` in `channels/mod.rs`.
- [ ] (Optional) Cargo feature in `gproxy-provider/Cargo.toml` and
  `gproxy-sdk/Cargo.toml`.
- [ ] (Optional) Pricing JSON under `channels/pricing/`.
- [ ] (Optional) Structured editor in `frontend/console`.
- [ ] Smoke test against a real upstream via the scoped path.
