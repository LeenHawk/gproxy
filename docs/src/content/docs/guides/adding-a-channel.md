---
title: Adding a Channel
description: "How a v4 channel is structured: the BaseChannel contract, the optional ability traits, the module layout, and the nine steps from a Cargo feature to a registered channel."
---

A channel knows one upstream family: its URLs, how a credential is injected,
which wire dialects it speaks, how its stream reports usage and — for
account-style upstreams — how to log in, refresh and read quota.

Everything else is somebody else's job. Routing, credential selection,
failover, protocol conversion, settlement and capture belong to the engine, and
**nothing in a channel reads the database or picks a transport**.

Channels are built in, not plugged in: a new one is a module of
`gproxy-channel` behind its own Cargo feature, and adding one is a pull request
against this repository. No concrete channel is compiled by default.

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
  from the credential. A provider's `allowed_headers` narrows what else is
  forwarded, while `content-type` and the channel's declared identity headers
  always pass.
- **An endpoint override is the complete method URL**, replacing the base URL
  and the channel's default path — not a base to append to.
- **A non-2xx is a response, not an error.** Status, headers and a lazy body
  come back as they stand. The error variant is for *ability* calls (login,
  refresh, quota) and for an intermediate exchange inside a multi-call
  override, where the failing reply is not the response being returned.
- **`RefreshRejected` means the upstream definitively refused the credential**
  (`invalid_grant`, revoked) and the host marks it dead. A transient transport
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
| `credential_refresh` | Produce a **full replacement** secret and expiry; the host persists it under a compare-and-swap on `version` |
| `oauth_authorization_code` | Build the authorize URL for a host-supplied PKCE challenge and state; exchange the code |
| `oauth_device_code` | `start` and one `poll` step. Scheduling belongs to the host |
| `cookie_login` | Exchange a pasted cookie for a credential plus its public metadata |
| `quota_model` | Synchronous and pure: which quota dimensions a credential has, from its `auth_kind` and metadata |
| `quota_query` | Read a quota snapshot from the upstream's usage endpoint |
| `quota_headers` | Turn response headers into quota entries; empty means nothing reported |
| `quota_reset` | Credits and a manual reset, on upstreams that sell them |
| `usage_extractor` | Normalized usage from a buffered response. `None` means **unreported**, not zero |
| `usage_stream` | A per-response observer fed raw chunks or frames. It never rewrites the delivered stream |
| `services` | Vendor control-plane routes — see [CLI Clients](/guides/cli-clients/) |

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

9. **Register it in the host.** The engine never lists channels itself. A host
   that wants everything it compiled in takes the whole list — which is what
   `gproxy-sdk` does, so enabling the feature there is the only step — and one
   that wants an exact set passes instances. A duplicate id is rejected, and a
   provider naming an unregistered channel is a configuration error rather than
   a fallback.

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
