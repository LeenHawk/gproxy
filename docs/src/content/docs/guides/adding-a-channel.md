---
title: Adding a Channel
description: "How a v4 channel is structured: the BaseChannel contract, the optional ability traits, the steps from a Cargo feature to a registered channel, and how to ship one from outside this repository."
---

A channel knows one upstream family: its URLs, how a credential is injected,
which wire dialects it speaks, how its stream reports usage and — for
account-style upstreams — how to log in, refresh and read quota.

Everything else is somebody else's job. Routing, credential selection,
failover, protocol conversion, settlement and capture belong to the engine, and
**nothing in a channel reads the database or picks a transport**.

A channel is compiled in, never loaded at runtime, and there are two places
it can live:

- **In this repository**: a module of `gproxy-channel` behind its own Cargo
  feature, added by a pull request. It reaches every host, the Workers one
  included. This is [the nine steps](#the-nine-steps).
- **In your own crate**: a type implementing `BaseChannel` against the public
  contract, and a native binary or a Worker that is the stock one plus that
  type. Nothing in this repository changes. This is
  [outside this repository](#outside-this-repository).

Both implement the same trait the same way; only the registration differs. No
concrete channel is compiled by default.

## Does It Need One?

A channel is code to maintain against somebody else's wire. A vendor earns one
only when a provider row cannot state what it needs: a path that moves with the
operation, a body that must be rewritten, an account surface to read, a client
identity to present.

A vendor whose whole difference is an origin and a header is a `custom`
provider. See
[vendors that need no channel](/guides/providers/#vendors-that-need-no-channel).

## The Contract

`BaseChannel` is the only required trait and `id` is its only required method.
Every protocol operation has its own asynchronous method with a **default**
implementation: build the request with `prepare` (HTTP) or `prepare_connect`
(WebSocket), then hand it to the assigned client.

Both preparation hooks default to "unsupported", so a channel supports exactly
what it prepares or overrides, **never more**.

```text
the host selects provider, credential, client
  → ChannelBinding::new(&channel, provider, credential, client)
  → binding.send(OperationKey, WireRequest<HttpBody>)
    binding.connect(OperationKey, WireRequest<()>)
  → the named operation method on BaseChannel
  → default: prepare + client.send  /  prepare_connect + client.connect
    or the channel's own multi-call flow through the same client
```

What the host hands over is borrowed and public where it can be:

| Type | Holds |
| --- | --- |
| `ProviderView` | `id`, `channel`, optional `base_url`, and the JSON `config` the channel decodes into its own typed settings |
| `CredentialView` | `id`, `provider_id`, `auth_kind`, the JSON `secret` (no `Debug`, no `Serialize`), the public `metadata` the host recorded, `version`, `expires_at_ms` |
| `OperationContext` | both views, the operation's dialect, the `WireRequest`, an owned client, cross-request state, the host instance id and an optional endpoint override |
| `CredentialContext` | provider, credential and client, for refresh, quota and services |

### Rules every channel follows

- **Preparation is synchronous and pure**: no I/O, no hidden state, no client
  construction. An operation override may make several exchanges, but only
  through `context.client`, and may keep it to finish work after the response
  stream ends.
- **Source authentication never reaches the upstream.** The forwarding helper
  drops the hop-by-hop headers plus `host`, `content-length`, `authorization`,
  `x-api-key`, `x-goog-api-key` and `api-key`; the channel adds its own auth
  from the credential. The allowed client headers are the union of the global
  allow-list, provider `allowed_headers`, and the channel's declared headers;
  `content-type` always passes. Missing or empty lists add no entries.
- **An endpoint override is the complete method URL**, replacing the base URL
  and the channel's default path — not a base to append to.
- **A non-2xx is a response, not an error.** Status, headers and a lazy body
  come back as they stand. The error variant is for *ability* calls (login,
  refresh, quota) and for an intermediate exchange inside a multi-call
  override, where the failing reply is not the response being returned.
- **`RefreshRejected` means the upstream definitively refused the credential**
  (`invalid_grant`, revoked). The host marks it dead by default; a channel may
  override `persist_rejection` to remember a given rejection reason only for
  the current in-memory credential version, reloading durable material before another try.
  A transient transport
  failure or a 5xx must surface as a transport error so the host retries later.
  Getting this wrong kills a working account.
- **`ContinuationElsewhere`** says a live upstream connection is held by
  another host process. The host reroutes; nothing is wrong with the
  credential.

## The Optional Abilities

Each is an independent trait behind a default-`None` accessor. A channel
implements the ones its upstream has, and nothing couples them — a channel may
report quota windows without offering a reset.

| Accessor | Purpose |
| --- | --- |
| `credential_refresh` | Produce a **full replacement** secret, expiry and optional public metadata; the host persists it under a compare-and-swap on `version` |
| `oauth_authorization_code` | Build the authorize URL for a host-supplied PKCE challenge and state; exchange the code |
| `oauth_device_code` | `start` and one `poll` step. Scheduling belongs to the host |
| `quota_model` | Synchronous and pure: which quota dimensions a credential has, from its `auth_kind` and metadata |
| `quota_query` | Read a quota snapshot from the upstream's usage endpoint |
| `quota_headers` | Turn response headers into quota entries; empty means nothing reported |
| `quota_reset` | Credits and a manual reset, on upstreams that sell them |
| `usage_extras` | The vendor's own usage fields, read beside the standard usage the host already took from the shaped response |
| `response_reason_observer` | Classify a raw upstream response (rate limit, overload, refusal…). The default parses standard JSON and SSE; override only for a private wire format |
| `websocket_response_reason` | The same, for one complete incoming WebSocket message |
| `services` | Vendor control-plane routes — see [CLI Clients](/guides/cli-clients/) |

Existing secrets use the ordinary credential create/update API. Validate their format in `validate_credential_secret`; return `Some(0)` from `initial_credential_expiry` when network initialization is required, then implement it through `CredentialRefresh`. A refresh returns a full secret replacement and expiry, plus optional public metadata (`None` preserves it). The host persists all three under the same version CAS; channels must keep secrets out of metadata.

`CredentialRefresh::proactive_refresh_reason` controls request-time renewal.
The default checks access expiry within five minutes or refresh expiry within
one day. A channel may override this for its upstream's schedule, and implement
`refresh_with_reason` when its token endpoint accepts the reason for renewal.

Usage itself needs no ability: the host reads it from the standard response the
channel shapes, and a channel that reports none is **unreported**, not zero.

Two descriptor capabilities cannot be derived from an accessor, so a channel
that has them says so in its own `descriptor`: `websocket` (it overrides a
socket operation) and `paid_usage` (its `QuotaModel::allows_paid_usage` reads
the credential metadata flag `allow_paid_usage`, which is what makes the
console offer that switch).

Cross-request memory is scoped by the host to one provider and one credential,
and written under compare-and-swap. The binding default refuses every write, so
a channel that wants state has to be given it.

### The login pocket

Both OAuth flows carry a pocket for facts a channel needs on the far side of
the person's authorization: what `start` puts in comes back with the
authorization. Its shape is the channel's own — a non-standard device handle, a
client a login registered for itself.

**This pocket may carry secrets**, unlike the public `provider_fields` the host
publishes as credential metadata: it lives in the host's login session, which
is short-lived, cache-backed and never rendered, and it is gone when the login
ends. Anything that must outlive the login is returned from the exchange —
public facts as metadata, secret ones as provider secrets, which are sealed
with the tokens and never become metadata.

## The Nine Steps

Use `custom` (API key) and `codex` (OAuth account) as the two references.

1. **Declare the feature** in `crates/gproxy-channel/Cargo.toml`, listing only
   the optional dependencies the channel needs. A wasm-only dependency goes
   under the `cfg(target_arch = "wasm32")` target table.

   ```toml
   [features]
   # One-line description of the upstream and what the channel covers.
   acme = ["dep:base64", "dep:web-time"]
   ```

2. **Add one line** to the `channels!` list in `src/channels/mod.rs`: the
   feature, the module, and the value a host registers. That single line
   declares the module and puts the channel in `compiled_in()`, which is how
   every host finds it.

   ```rust
   "acme" => acme, acme::Acme;
   ```

   If the channel uses `shared`, extend that module's own `cfg(any(...))` too.

3. **Lay the module out one concern per file.** A small channel is a single
   `acme.rs`; a larger one a directory:

   ```text
   src/channels/acme/
     mod.rs        the id, the unit struct, re-exports, the module doc
     config.rs     the provider `config` JSON, serde(default), unknown keys ignored
     request.rs    prepare / prepare_connect / overridden operations
     oauth.rs      the login and refresh abilities
     quota.rs      the quota abilities
     usage.rs      the usage abilities
     services.rs   the vendor control-plane routes
   ```

   **The module doc names the source of every wire fact** — the vendor's API
   reference, or the CLI source under `samples/`. Wire truth is never another
   channel's code.

4. **Implement `BaseChannel`.** The minimum is `id`, `native_dialects` and
   `prepare`. The struct is a stateless unit, so one instance serves every
   provider of that channel.

   Override `descriptor` as well. The default reads the capability accessors,
   and only the channel knows its display name and which keys it decodes out of
   the provider `config` JSON. **That descriptor is all a management UI has to
   render a provider form from** — under the `ts` feature it is a TypeScript
   type too — so a key left out of it is a key nobody can set.

   Override an operation method instead of `prepare` when the upstream needs
   more than one exchange, a locally synthesized reply, a different transport,
   or a response rewritten into the declared native dialect.

5. **Declare what the vendor client sends.** If the channel impersonates a CLI,
   export its header constant and build the allow-list from it, so a provider's
   `allowed_headers` cannot strip the CLI's own headers. Pass those identity
   headers to the forwarding helper's drop list so a **client cannot spoof
   them**. Return a default connection when the upstream fingerprints its
   clients; an explicit profile on the credential or provider still wins.

6. **Add abilities as separate types** and return them from the accessors. Keep
   each trait's rules: a refresh returns a full replacement, never a merge;
   quota dimensions read plan facts from the credential's metadata and never
   the network; a usage observer snapshots cumulatively, and its `finish`
   receives complete-or-interrupted **from the host**, because EOF alone does
   not establish complete usage.

7. **Put reusable wire mechanics in `src/channels/shared/`**, gated on the
   features that use them. Policy stays in the channel; a shared module only
   executes what a channel asks for.

8. **Test beside the code** in `tests/<id>.rs` behind
   `#![cfg(feature = "…")]`. Build the provider and credential views by hand,
   call `prepare` with a fixture secret, and assert the URL, the injected auth
   and the dropped headers. Feed captured frames to the usage observer; parse
   recorded quota headers and usage bodies. A scripted outbound client
   exercises multi-call overrides. **Do not test the engine from here.**

9. **Forward the feature to every host.** The engine never lists channels
   itself: `gproxy-sdk` registers whatever `compiled_in()` returns, so a channel
   is reachable from a binary exactly when that binary's feature set reaches
   its feature. Add the feature, by the same name, to:

   | File | Line to add |
   | --- | --- |
   | `crates/gproxy-sdk/Cargo.toml` | `acme = ["gproxy-channel/acme"]`, plus `"acme"` in its `channels` list |
   | `crates/gproxy/Cargo.toml` | `acme = ["gproxy-sdk/acme"]` |
   | `crates/gproxy-host-edge/Cargo.toml` | `acme = ["gproxy-sdk/acme"]` |
   | `crates/gproxy-app/src/v3/channels.rs` | the id in `V4_CHANNELS`, so a v3 import accepts it |

   A duplicate id is rejected, and a provider naming an unregistered channel is
   a configuration error rather than a fallback.

## Finish

```sh
cargo fmt --all
cargo clippy -p gproxy-channel --all-features --all-targets -- -D warnings
cargo clippy -p gproxy-channel --all-features --target wasm32-unknown-unknown --lib -- -D warnings
cargo test   -p gproxy-channel --all-features
```

**Every channel builds for native targets and for
`wasm32-unknown-unknown`**, which is what keeps the Workers host from being a
reduced channel set. A lint finding gets a code change, not an `#[allow]`.

Once it is in, nothing else is required: a provider on the new channel appears
in `GET /admin/api/channels` with its descriptor, and a management UI renders
its form from that.

## Outside This Repository

A channel can live in a crate of its own and ship without a fork, in either
host: a native binary that is the stock `gproxy` — same commands, flags,
console and database — or a Worker that is the stock GPROXY Worker, each with
the channel registered beside the built-in ones. `examples/channel-plugin` is
a complete, working set of all three crates: an OpenAI-compatible API-key
channel in one file, and a few-line entry point for each host.

### The layout

```text
my-gproxy/
  channel/    the channel: a unit struct implementing BaseChannel, no host
  native/     the gproxy binary, carrying it
  edge/       the Cloudflare Worker, carrying it (only if you deploy there)
```

The channel is its own crate so both hosts can depend on it. The native host
cannot be one of its dependencies: `gproxy` does not build for wasm, and the
Worker would not build either.

```rust
// native/src/main.rs
use std::{process::ExitCode, sync::Arc};

fn main() -> ExitCode {
    gproxy::main_with(vec![Arc::new(my_channel::Acme)])
}
```

`main_with` is the whole stock `main`: it loads `.env`, parses the same
arguments, builds the runtime and runs. A host with a runtime of its own calls
`gproxy::run_with(cli, channels)` instead.

```rust
// edge/src/lib.rs
#[cfg(target_arch = "wasm32")]
mod entry {
    use std::sync::Arc;

    use worker::{Context, Env, HttpRequest, event};

    #[event(fetch)]
    async fn fetch(
        request: HttpRequest,
        env: Env,
        ctx: Context,
    ) -> worker::Result<gproxy_host_edge::Response> {
        gproxy_host_edge::fetch_with(request, env, ctx, || vec![Arc::new(my_channel::Acme)])
            .await
    }
}
```

`fetch_with` is the whole stock fetch handler. A Worker module has exactly one
`#[event(fetch)]`, so the edge crate depends on `gproxy-host-edge` **without**
its `entry` feature, which is the one that declares the stock handler. The
closure runs once per isolate, when the first request assembles it.

In both hosts a channel whose `id` matches a built-in one **replaces** the
built-in, which is how a patched copy of an existing channel ships.

### Dependencies

Depend on a gproxy checkout by path, at the version you build against:

```toml
# channel/Cargo.toml
[dependencies]
gproxy-channel = { path = "../../gproxy/crates/gproxy-channel" }
gproxy-protocol = { path = "../../gproxy/crates/gproxy-protocol", features = ["exhaustive"] }
http = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

```toml
# native/Cargo.toml
[dependencies]
my-channel = { path = "../channel" }
gproxy = { path = "../../gproxy/crates/gproxy", features = ["channels", "memory", "fs", "embedded-console"] }
```

```toml
# edge/Cargo.toml
[lib]
crate-type = ["cdylib", "rlib"]

[target.'cfg(target_arch = "wasm32")'.dependencies]
my-channel = { path = "../channel" }
gproxy-host-edge = { path = "../../gproxy/crates/gproxy-host-edge", default-features = false, features = ["channels", "d1", "libsql"] }
worker = { version = "0.8", default-features = false, features = ["http"] }
```

```toml
# Cargo.toml, the workspace root
[workspace]
resolver = "3"
members = ["channel", "native", "edge"]

# Copied from the checkout's root Cargo.toml: a patch applies only in the
# workspace that declares it, and the build depends on these.
[patch.crates-io]
sqlx-sqlite = { path = "../gproxy/vendor/sqlx-sqlite" }
btls-sys = { git = "https://github.com/0x676e67/btls", rev = "609f955e6de4df4a215ee9b273d327af1c0771db" }
```

Copy the checkout's `Cargo.lock` next to the root `Cargo.toml` before the first
build, so you resolve the dependency versions gproxy was tested with.

A checkout rather than a git dependency, for one reason: the native console is
built into the binary from `crates/gproxy-host-axum/assets/web`, which is not
committed. Run `pnpm install && pnpm build` in the checkout's `console/` first,
or leave `embedded-console` out of the features for a binary without one. The
Worker build below takes its console from the checkout too.

`gproxy`'s and `gproxy-host-edge`'s own feature lists work as they do for the
stock builds: `channels` is every built-in channel, or name only the ones a
deployment uses.

### Building the Worker

The checkout's `deploy/cloudflare` builds any Worker crate, not only the stock
one. Point it at yours with an absolute path and deploy as usual:

```sh
cd ../gproxy/deploy/cloudflare
pnpm install
GPROXY_EDGE_CRATE=/absolute/path/to/my-gproxy/edge pnpm run build
pnpm run deploy
```

That runs `worker-build` in your crate, strips the debug sections and copies
the Worker next to the checkout's `wrangler.toml`, the console assets and their
headers. Everything in the [edge deployment guide](/deployment/edge/) — the D1
binding, secrets, `GPROXY_CONFIG` — applies unchanged.

**A channel for the Worker must build for `wasm32-unknown-unknown`**, the rule
every built-in channel already follows. In practice: no `tokio` networking, no
filesystem, `web-time` rather than `std::time::Instant`, and every request
through the client the host hands over. Check it with
`cargo check --target wasm32-unknown-unknown -p my-channel`.

### What the public contract gives you

Everything a channel implements or receives is public in
`gproxy_channel::channel`: `BaseChannel`, the context and view types, every
ability trait, the descriptor types, and the helpers a request needs —
`forwardable` and `HeaderAllowlist` for the client headers, `insert_configured`
for an operator's static header, `strip_query_auth` for the query string, and
`standard_reason_observer`. The protocol vocabulary (`Dialect`, `Operation`,
`OperationKey`, `HttpBody`, `WireRequest`) is `gproxy_protocol`'s.

`gproxy_channel::channels::shared` is **not** public: it is tuned to the
channels in this repository and changes with them. Copy what you need.

The rules above apply unchanged — especially that preparation is pure and
source authentication never reaches the upstream.

### What differs from a built-in channel

- **The console renders your channel from its descriptor**, like any other: the
  provider form from `config_keys`, the credential form from
  `credential_kinds`, the login buttons from `login_modes`, the quota, reset,
  refresh and paid-usage controls from `capabilities`. What it does not have is
  the per-channel polish written for built-in ids — a tailored hint text, a
  model preset group.
- **Self-update refuses to install** in the native binary. A published release
  is this repository's build; swapping it in would drop your channel and break
  every provider on it at the next restart. Rebuild against the new version
  instead. Checking for updates still works.
- **The contract follows gproxy's version.** Changes are additive in practice —
  most protocol types are `#[non_exhaustive]`, and new `BaseChannel` methods
  come with defaults — but build against the version you run. Write
  `ChannelCapabilities { quota_query: true, ..Default::default() }` rather than
  listing every field, so a new capability does not break your build.

## Checklist for Coding Agents

The same steps, as an agent should execute them.

**Built in** (a pull request here):

1. Read `crates/gproxy-channel/src/channels/nvidia.rs` (API key) or `codex/`
   (OAuth) as the template, and the vendor's wire source under `samples/` or
   its API reference.
2. Add the feature to `crates/gproxy-channel/Cargo.toml`, the line to
   `channels!` in `src/channels/mod.rs`, and the feature to each `cfg(any(...))`
   in `src/channels/shared/` the channel uses.
3. Write `src/channels/<id>.rs` or `src/channels/<id>/`; override `descriptor`.
4. Forward the feature in the four places of step 9.
5. If the channel honours `allow_paid_usage` or serves a WebSocket, set
   `paid_usage` / `websocket` in its descriptor's capabilities.
6. Add translations for new descriptor/schema descriptions to
   `docs/src/i18n/reference/zh-cn.json` and `zh-tw.json`. Run
   `pnpm --dir docs generate:reference`; it exports the registered channels
   and rejects untranslated descriptions. Do not commit generated JSON.
7. Run the commands under [Finish](#finish).

**Outside this repository**:

1. Copy `examples/channel-plugin` out of the checkout — `channel/` always,
   `native/` and `edge/` for the hosts you deploy. Add the workspace root
   `Cargo.toml` above with its `[patch.crates-io]` table, and copy `Cargo.lock`.
2. In each crate's `Cargo.toml`, give the package its own `version` and
   `edition = "2024"` in place of the `*.workspace = true` keys, and turn every
   `workspace = true` dependency into the path dependency shown above.
3. Rename the packages, the binary, the struct and `ID`. Choose an id no
   built-in uses unless the point is to replace that one.
4. Implement against `gproxy_channel::channel` only; do not reach for
   `channels::shared`.
5. `cargo clippy --all-targets -- -D warnings`; for the Worker also
   `cargo check --target wasm32-unknown-unknown -p <channel> -p <edge>`.
6. Run the native binary on a fresh `GPROXY_DATA_DIR`, or the Worker under
   `wrangler dev --local` from `deploy/cloudflare` after the build above, and
   check that `GET /admin/api/channels` lists the id.
