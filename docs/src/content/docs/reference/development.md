---
title: Development Reference
description: Build commands, workspace layout, channel contribution guide, and data directories.
---

## Commands

### Backend

```bash
cargo fmt --all --check
cargo check
cargo clippy --workspace --all-targets -- -D warnings -A clippy::too_many_arguments
cargo test --workspace --all-targets --no-fail-fast
cargo run -p gproxy
```

CI runs exactly these commands. `clippy` treats all warnings as errors (`-D warnings`) except `too_many_arguments`.

### Frontend

```bash
cd frontend/console
pnpm install
pnpm typecheck
pnpm test
pnpm build
```

The built console is embedded into the Rust binary via `rust-embed` at `apps/gproxy/web/console`. Run `pnpm build` before `cargo build --release` to include the latest frontend.

### Docs

```bash
cd docs
pnpm install
pnpm build
```

Docs use Starlight (Astro-based). Dev server: `pnpm dev`.

## Workspace layout

| Path | Package | Description |
|---|---|---|
| `sdk/gproxy-sdk` | `gproxy-sdk` | Re-export crate aggregating provider + protocol + routing SDKs |
| `sdk/gproxy-protocol` | `gproxy-protocol` | Protocol definitions (operation families, protocol kinds, transforms) |
| `sdk/gproxy-provider` | `gproxy-provider` | Channel trait, dispatch tables, all channel implementations |
| `sdk/gproxy-routing` | `gproxy-routing` | Model routing, provider prefix handling, model alias resolution |
| `crates/gproxy-core` | `gproxy-core` | Shared core types and utilities |
| `crates/gproxy-storage` | `gproxy-storage` | Database layer (SeaORM, supports SQLite/MySQL/PostgreSQL) |
| `crates/gproxy-api` | `gproxy-api` | HTTP API layer (Axum router, auth, admin/user/provider handlers) |
| `crates/gproxy-server` | `gproxy-server` | Server runtime (AppState, middleware, in-memory caches, sessions) |
| `apps/gproxy` | `gproxy` | Main binary (CLI, config, web serving, startup) |
| `apps/gproxy-recorder` | `gproxy-recorder` | MITM recording proxy for provider traffic capture |
| `frontend/console` | `@gproxy/console` | Admin console SPA (SolidJS + TypeScript) |
| `docs` | - | Documentation site (Starlight) |

## Contributing a channel

v1 uses trait-based channels with automatic registration via the `inventory` crate. No manual enum wiring needed.

### Steps

1. Create `sdk/gproxy-provider/src/channels/your_channel.rs` (or a directory with `mod.rs` for larger channels).

2. Implement the `Channel` trait:

```rust
use gproxy_provider::channel::{Channel, ChannelSettings, ChannelCredential};
use gproxy_provider::dispatch::DispatchTable;
use gproxy_provider::health::CredentialHealth;
use gproxy_provider::request::PreparedRequest;
use gproxy_provider::response::{ResponseClassification, UpstreamError};

pub struct YourChannel;

impl Channel for YourChannel {
    const ID: &'static str = "your_channel";

    type Settings = YourSettings;
    type Credential = YourCredential;
    type Health = YourHealth;

    fn dispatch_table(&self) -> DispatchTable { /* ... */ }

    fn prepare_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
        request: &PreparedRequest,
    ) -> Result<http::Request<Vec<u8>>, UpstreamError> { /* ... */ }

    fn classify_response(
        &self,
        status: u16,
        headers: &http::HeaderMap,
        body: &[u8],
    ) -> ResponseClassification { /* ... */ }
}
```

3. Implement associated types:
   - `Settings` -- implements `ChannelSettings` (base URL, user agent, retry config)
   - `Credential` -- implements `ChannelCredential` (API key, OAuth tokens)
   - `Health` -- implements `CredentialHealth` (alive/dead/cooldown tracking)

4. Implement `dispatch_table()` returning route mappings from (operation, protocol) pairs to channel behavior.

5. Implement `prepare_request()` to build the upstream HTTP request (set URL, auth headers, body).

6. Implement `classify_response()` to decide retry behavior based on upstream response (success, rate limit, auth failure, server error).

7. Register with inventory at module scope:

```rust
inventory::submit! {
    ChannelRegistration::new(YourChannel::ID, your_dispatch_table)
}
```

8. Add `mod your_channel;` to `sdk/gproxy-provider/src/channels/mod.rs`.

That's it. No manual enum registration, no provider.rs wiring, no settings.rs plumbing. `inventory` handles discovery at link time.

### Optional trait methods

The `Channel` trait has several optional methods with default implementations:

- `finalize_request()` -- body normalization before credential selection
- `normalize_response()` -- fix non-standard upstream response fields
- `count_strategy()` -- override token counting (default: local tiktoken)
- `handle_local()` -- handle requests without upstream call
- `needs_spoof_client()` -- use browser-impersonating HTTP client
- `ws_extra_headers()` -- extra WebSocket handshake headers
- `refresh_credential()` -- refresh credentials after auth failure
- `prepare_quota_request()` -- build upstream quota query request
- `oauth_start()` / `oauth_finish()` -- OAuth flow support
- `model_pricing()` -- default pricing table

### Frontend integration

If the channel needs custom admin UI (settings form, credential form):

1. Add channel files under `frontend/console/src/modules/admin/providers/channels/your_channel/`.
2. Register the channel in the frontend channel registry.

### Validation

```bash
cargo check
cargo clippy --workspace --all-targets -- -D warnings -A clippy::too_many_arguments
cargo test --workspace --all-targets --no-fail-fast
```

## Data directories

| Path | Description |
|---|---|
| `./data` | Default data directory |
| `./data/gproxy.db` | Default SQLite database (`sqlite://./data/gproxy.db?mode=rwc`) |
| `./data/tokenizers` | HuggingFace tokenizer cache (downloaded on first use) |

The `dsn` config field can switch to MySQL or PostgreSQL. SQLite is the default for single-instance deployments.
