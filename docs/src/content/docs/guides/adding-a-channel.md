---
title: Adding a Channel
description: Implement a built-in adapter or a native compile-time external channel crate and link it into a custom GPROXY binary.
---

A channel is the upstream access adapter. It injects authentication, resolves
the endpoint URL, classifies upstream responses, and optionally handles
provider-specific request, response, stream, login, refresh, and usage behavior.
It does not own cross-protocol transforms or provider rule-set processing.

```text
transform/     protocol conversion by Operation
process/       provider rule sets after transform
channel/       upstream access, auth, endpoint, response disposition
```

:::caution
An external channel is trusted Rust code linked into a custom **native** GPROXY
binary at compile time. It is not a runtime-loaded plugin, shared library,
sandbox, or hot-reload mechanism. Official GPROXY binaries contain only the
channels compiled by the official build.
:::

## Choose an Integration Path

| Path | Use it for | Registration | Targets |
| --- | --- | --- | --- |
| External crate | Private or out-of-tree adapters without changing GPROXY source. | A `linkme` constructor collected when the native process starts. | Native only. |
| Built-in channel | Upstream contributions, official distribution, or edge support. | An explicit root registry entry and Cargo channel feature. | Native and, when compatible, edge. |

Both paths implement the same `gproxy-channel-api::Channel` contract and run
through the same routing and execution pipeline. The external path requires a
small custom runner because adding a crate changes the final executable.

## Start from a Similar Channel

Built-in channels under `src/channel/bulletins/` are useful references:

| Upstream shape | Starting point |
| --- | --- |
| OpenAI-compatible API key | `openai`, `custom`, `deepseek`, `groq`, `nvidia` |
| Anthropic Messages | `claudeapi` |
| Gemini API key | `aistudio`, `vertexexpress` |
| Vertex service account | `vertex` |
| OAuth or agent envelope | `codex`, `claudecode`, `geminicli`, `antigravity`, `kiro`, `copilotcli` |

External crates should copy the concepts, not imports from private root modules.
Depend on `gproxy-channel-api` and use its `protocol` and `transform` re-exports
so the channel and host share the exact same public types.

The checked example is in `examples/external-channel/`:

```text
external-channel/
|-- channel/     # MIT adapter, depends only on gproxy-channel-api
`-- app/         # AGPL custom runner, links gproxy and the adapter
```

## Create the External Crate

The adapter needs the public API plus a direct `linkme` dependency for the
registration attribute:

```toml
[package]
name = "my-gproxy-channel"
version = "0.1.0"
edition = "2024"

[dependencies]
gproxy-channel-api = { version = "=2.4.0", features = ["external-channels"] }
http = "1"
linkme = "0.3"
```

Use the API release that matches the GPROXY source or tag used by the custom
runner. Cargo package identity includes the dependency source, not only its name
and version. If the adapter resolves `gproxy-channel-api` from crates.io while
the host resolves a separate Git or path copy, the program can compile but
register into a different linker slice. Use one checkout for both during local
development, or patch crates.io to the same GPROXY tag in an out-of-tree
workspace:

```toml
[patch.crates-io]
gproxy-channel-api = {
  git = "https://github.com/LeenHawk/gproxy",
  tag = "vX.Y.Z",
}
```

Confirm that the runner has only one package identity:

```bash
cargo tree -p my-gproxy-runner -i gproxy-channel-api
```

## Implement `Channel`

The three required methods are:

| Method | Responsibility |
| --- | --- |
| `id()` | Stable registry id; it becomes `Provider.channel`. |
| `routing_table()` | Declared `(Operation, OperationKind) -> RoutingDecision` surface. |
| `prepare()` | Build an absolute upstream request and inject auth. |

Important optional hooks are:

| Hook | Use when |
| --- | --- |
| `metadata()` | The Admin API and generic Console forms need names, settings, credentials, login modes, endpoints, or usage capabilities. |
| `classify()` | Upstream status or headers need provider-specific retry, cooldown, or auth-dead handling. |
| `shape_request()` | The provider-native body needs hygiene after transform and process rules. |
| `shape_response()` | The raw upstream body needs normalization before response transform. |
| `stream_decoder()` | An envelope or binary stream must be decoded before SSE transformation. |
| `needs_refresh()` / `refresh()` | OAuth-like credentials must be refreshed before use. |
| `prepare_usage_request()` / `parse_usage()` | The provider exposes a per-credential usage or quota endpoint. |

`prepare()` receives the body after protocol transform and configured process
rules. Move `ctx.body` into the request and use an absolute URI. Do not copy all
downstream headers: build an allow-list and inject the upstream credential
explicitly. Put provider-native body cleanup in `shape_request()` instead.

The host owns persistence, secret encryption, refresh leases, proxy selection,
TLS/HTTP client resolution, and request capture. A channel receives the resolved
`UpstreamClient` capability for async refresh, login, and custom multi-step
exchanges, so those flows can make arbitrary HTTP, WebSocket, or streaming calls
without opening their own unmanaged client.

See `examples/external-channel/channel/src/lib.rs` for a complete API-key
adapter that resolves settings, injects bearer auth, preserves an allow-list of
headers, declares routes, and supplies runtime metadata.

## Declare Operation Routing

Use the public route helpers and protocol re-export:

```rust
use gproxy_channel_api::protocol::{
    ContentGenerationKind::*, Operation::*, Provider as P,
};
use gproxy_channel_api::routes::{cg, pass, pv, xform};

vec![
    pass(ListModels, pv(P::OpenAi)),
    xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
    pass(GenerateContent, cg(OpenAiChatCompletions)),
    xform(
        GenerateContent,
        cg(ClaudeMessages),
        GenerateContent,
        cg(OpenAiChatCompletions),
    ),
]
```

Routing is operation-first. Each cell says whether this channel can serve that
operation and wire kind by passthrough, transform, local handling, or an
explicit unsupported decision. Provider creation materializes the list into
stored `routing_rules`; a missing row is unsupported at runtime.

An external crate can use the existing provider families, operations, and
transform topology. Adding a new core enum variant or protocol family still
requires a coordinated built-in change because those enums are shared API.

## Add Console Metadata

`Channel::metadata()` drives authenticated `GET /admin/channels` and the
Console's generic provider and credential forms. External catalog entries are
marked with `source: "external"`; the Console uses static overlays only for
built-in channels.

| `ChannelMetadata` field | Purpose |
| --- | --- |
| `id`, `display_name` | Registry identity and operator-facing name. |
| `credential_family` | API key, OAuth tokens, service account, or GitHub token. |
| `login_modes` | Auth-code, device-code, or cookie flows supplied by `ChannelLogin`. |
| `settings_fields` | Generic provider settings rendered by the Console. |
| `secret_template` | JSON shape used when creating a credential. |
| `endpoint_kinds` | Exact endpoint override fields the adapter understands. |
| `usage` | Whether live credential usage is available. |

Settings support `text`, `url`, `boolean`, `integer`, and `string_list`
controls. `ChannelMetadata::new(id)` supplies a minimal API-key default;
override fields for richer configuration. External channels should not edit the
Console's static built-in `CHANNELS` table.

If the channel offers interactive login, implement `ChannelLogin`. All login
contexts include provider settings, and successful secrets still go through the
host's normal encryption and persistence path.

## Register the Channel

### External compile-time registration

Export a native constructor into the API crate's distributed slice:

```rust
use std::sync::Arc;
use gproxy_channel_api::{ChannelRegistration, RegisteredChannel};

#[cfg(not(target_arch = "wasm32"))]
fn register() -> RegisteredChannel {
    RegisteredChannel::new(Arc::new(MyChannel))
}

#[cfg(not(target_arch = "wasm32"))]
#[linkme::distributed_slice(
    gproxy_channel_api::registration::CHANNEL_REGISTRATIONS
)]
static REGISTER: ChannelRegistration = register;
```

Use `RegisteredChannel::with_login(...)` when the same id also has a
`ChannelLogin` implementation. Registration is startup-only. A duplicate
channel or login id fails startup rather than overriding a built-in or depending
on linker order.

### Built-in source registration

For a built-in contribution, add the module under
`src/channel/bulletins/mod.rs`, add its `channel-*` Cargo feature to the proper
native or edge umbrella, and add it to `builtin_channels()` in
`src/channel/registry.rs`. Add interactive login to `builtin_logins()`, metadata
to `src/channel/metadata.rs`, and a host-owned emulation profile to
`src/channel/emulation.rs` only when those capabilities are needed.

## Build a Custom Native Binary

The final binary must name the channel crate so the linker retains its
registration section:

```rust
use my_gproxy_channel as _;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    gproxy::native::run_cli().await
}
```

`gproxy::native::run_cli()` preserves the standard CLI, migrations, bootstrap,
storage, Console server, and native channel collection. A custom runner links
the AGPL `gproxy` application; keep its licensing and source-distribution
obligations separate from the MIT-only channel API crate.

The repository example compiles and tests the complete arrangement:

```bash
cargo test \
  --manifest-path examples/external-channel/Cargo.toml \
  -p gproxy-example-channel-bin \
  --test linked_registration

cargo build \
  --manifest-path examples/external-channel/Cargo.toml \
  -p gproxy-example-channel-bin
```

For an out-of-tree runner, depend on the root `gproxy` package from one pinned
Git tag or checkout, use the same source for the API patch, and rebuild the
runner when upgrading GPROXY. General native build and Console asset details are
in [Release Build](/deployment/release-build/).

## Verify the Compiled Channel

1. Start the custom runner with the normal GPROXY configuration.
2. Authenticate as an administrator and request `GET /admin/channels`.
3. Confirm the entry has the expected id and `source: "external"`.
4. Confirm the Console provider selector and generic fields match `metadata()`.
5. Create a provider and inspect the routing rules seeded from `routing_table()`.
6. Send one provider-scoped request before adding it to production routes.

## Limits and Upgrades

- Compile-time external registration is native-only. Edge builds use the
  explicit built-in registry.
- External code runs in-process with the same privileges as GPROXY. There is no
  ABI isolation or sandbox.
- Channels cannot be loaded, unloaded, or upgraded without rebuilding and
  restarting the binary.
- Official binaries, containers, and edge bundles do not gain an out-of-tree
  adapter. Distribute the custom runner instead.
- Applying GPROXY's official self-update to a custom runner can replace it with
  an official binary that lacks the external crate. Rebuild and deploy the
  custom runner for upgrades.
- If the channel is absent, first check `use my_gproxy_channel as _;`, then use
  `cargo tree` to verify there is only one `gproxy-channel-api` package source.
