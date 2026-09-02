---
title: Adding a Channel
description: "How a built-in channel is structured, the Channel contract it implements, where it is registered, and what the console picks up without new UI"
---

A channel is the adapter for one upstream family: it knows the URLs, how to
inject a credential, how to read a stream, how to extract usage and, for
OAuth-style upstreams, how to log in and refresh. Everything else, routing,
admission, failover, transforms, settlement and capture, is the engine's job
and is not reimplemented in a channel.

## Built In, Not Plugged In

v3 has no plugin mechanism: no `linkme` slice, no external channel crate,
no Cargo feature per channel. Every channel is a module of
`crates/gproxy-channels`, and the set compiled into the binary is the list
in `crates/gproxy-app/src/bootstrap.rs`. Adding a channel means a pull
request against this repository. The 28 built-in ids form the runtime
catalogue; `claudeweb` is the only one gated to native builds.

## The `Channel` Contract

The contract lives in `crates/gproxy-channel-api/src/`. `Channel` is
synchronous and object-safe: an adapter is pure logic over borrowed data,
and the one async concern, credential refresh, returns a boxed future.
`prepare` must not perform I/O.

Required methods:

| Method | Responsibility |
| --- | --- |
| `descriptor()` | The identity card: `id`, `display_name`, executable `supports`, declared `provider_fields` and `credential_fields`, `endpoint_overrides`, `traffic_policy`. |
| `routing_table()` | The provider defaults seeded on provider creation: one `ChannelSupport` per (operation, inbound protocol) with action `passthrough`, `transform`, `local` or `unsupported`. |
| `prepare(PrepareCtx)` | Build the absolute upstream request: URL from settings or an endpoint override, auth injected from the decrypted secret, header allow-list, body shaping. |
| `classify(ResponseView)` | Map an upstream answer to `Success`, `Retryable`, `Terminal` or `CredentialDead`; this drives failover and health. |
| `extract_usage(UsageCtx)` | Read `NormalizedUsage` from a buffered exchange: input, output and cached tokens plus dimensional `metrics` and `dimensions`. |

Optional hooks, whose defaults do nothing:

| Hook | Use when |
| --- | --- |
| `login()` | The channel acquires credentials interactively. Returns a `ChannelLoginRef` whose descriptor lists modes (`AuthCode` with PKCE, `Device`, `Cookie`) and parameters; the adapter implements `ChannelLogin`. |
| `refresh_due()`, `refresh()` | The secret expires. `refresh` returns the full replacement secret; the engine persists it through the host's version-guarded store. |
| `stream_decoder(StreamCtx)` | The wire is SSE, AWS event-stream or another framing that must be decoded into frames and observed for usage. Returns a `StreamDecoder` state machine: `push` chunks in, `Frame`s out, `finish` yields the `StreamTail` with usage. |
| `shape_response()` | A channel-private envelope must be normalised to the declared native wire before the outward transform. |
| `select_support()` | One credential family serves several source rows and the secret shape decides which. |
| `operation_driver()` | An operation needs several upstream calls (create, poll, fetch). The driver is a state machine; the core performs and funnels every call. |
| `observe_quota()`, `prepare_quota_probe()`, `parse_quota_probe()` and the credits and reset variants | The upstream reports quota windows in headers or exposes a usage endpoint. |
| `session_preparer()` | Long-lived realtime sessions with a usage meter. |
| `settlement_ready()`, `resource_mutations()` | Asynchronous operations and durable resources (files, videos) whose ownership must be recorded. |
| `surfaces()`, `prepare_surface()` | The channel emulates a vendor control plane (Codex `backend-api`, Claude Code files). Entries are rows: method, path pattern, credential affinity, and forward or synthesize. |
| `requires_continuations()` | The channel relies on continuation state between calls. |

`PreparedRequest` carries the request plus an optional stream `framing`
override, a `websocket` flag and an optional `ClientProfile`: the TLS and
HTTP/2 fingerprint a native transport applies
(`ClientProfilePreset::Chrome148` is the captured preset). Edge hosts
ignore optional profiles.

## Declared Fields

The console has no channel-specific screens. Everything it renders for a
channel comes from the descriptor through `GET /admin/api/channels`:

| Field | Purpose |
| --- | --- |
| `provider_fields` | Typed provider settings. Controls: `text`, `secret`, `url`, `integer`, `boolean`, `string_list`, `select` (with `options` and `default_value`); `required` and `advanced` flags. |
| `credential_fields` | The secret's shape when a credential is pasted: `api_key`; `access_token` and `refresh_token`; service-account fields. |
| `endpoint_overrides` | Whether the Settings tab offers per-operation endpoint URL overrides; the keys come from `endpoint_override_key`. |
| `traffic_policy` | Request headers, response headers and query parameters the channel forwards; operators may override them per provider. |
| `login` | Modes and parameters for the credential wizard. |

Reuse the field sets in `crates/gproxy-channels/src/metadata.rs`
(`BASE_URL`, `OPENAI_CACHE`, `CLAUDE`, `API_KEY`, `OAUTH`,
`SERVICE_ACCOUNT` and others) where they fit. Labels come from the locale
files: each field key needs `providers.channelFields.<key>.label` and
`.description` in `console/src/locales/{en,zh-CN,zh-TW}/providers.json`,
and `select` options need `providers.channelFieldOptions.<key>.<option>`.
The admin API adds `auto_refresh_models` to every channel itself.

## Routing Table

Declare routes with the `route!` macro from `shared/routing.rs`:

```rust
use crate::shared::routing::route;

pub(super) static ROUTES: &[ChannelSupport] = &[
    route!(pass ListModels, openai),
    route!(xform ListModels, claude => ListModels, openai),
    route!(local CountTokens, openai),
    route!(pass GenerateContent, openai_chat),
    route!(xform GenerateContent, claude_messages => GenerateContent, openai_chat),
    route!(unsupported CreateEmbedding, gemini),
];
```

Wire kinds are `openai`, `claude`, `gemini` for family operations and
`openai_chat`, `openai_responses`, `openai_responses_websocket`,
`claude_messages`, `gemini_generate_content` for content generation. An
`xform` row must name a pair the transform registry implements; the test
`every_declared_builtin_transform_is_wired` in
`crates/gproxy-core/src/tests/channels.rs` checks the descriptor's
`supports` for that, and a new channel with transforms belongs in its list.
Operators can override any row per provider afterwards
(see [Routing Rules & Rule Sets](/guides/rules/)).

## Where a Channel Lives

One directory per channel id, one file per concern, no file over 500 lines
and ideally under 200:

```text
crates/gproxy-channels/src/<id>/
  mod.rs        descriptor, SUPPORTS, Channel impl
  routes.rs     routing_table()
  prepare.rs    URL, auth, endpoint overrides
  model.rs      model id and body shaping
  sse.rs        stream decoder
  usage.rs      usage extraction
  resource.rs   settlement_ready / resource_mutations (when needed)
  login.rs      ChannelLogin (when needed)
  auth.rs       refresh_due / refresh (when needed)
  quota.rs      quota probe (when needed)
  surface/      service-surface table and synthesizers (when needed)
  tests.rs      or tests/ for larger suites
```

Shared wire knowledge goes under `crates/gproxy-channels/src/shared/`:
`openai`, `claude`, `gemini`, `aws_eventstream`, `code_assist`,
`google_oauth`, `cache` (magic strings), `quota`, `http`,
`image_multipart`. `policy.rs` holds each channel's `ChannelTrafficPolicy`,
`metadata.rs` the field sets, and `legacy.rs` canonicalises settings
imported under older ids.

Read `crates/gproxy-channels/src/openai/` for an API-key channel and
`claudecode/` for an OAuth channel with login, refresh and surfaces. Wire
truth is the vendor's API documentation, not another channel's code.

## Registration

1. Add `mod <id>;` and `pub use <id>::<Name>Channel;` in
   `crates/gproxy-channels/src/lib.rs`.
2. Push `Box::new(gproxy_channels::<Name>Channel)` onto the list in
   `channels()` in `crates/gproxy-app/src/bootstrap.rs`.
   `ChannelRegistry::new` fails startup on a duplicate id.
3. If the channel cannot build for `wasm32-unknown-unknown`, gate the module
   and the registration with `#[cfg(not(target_arch = "wasm32"))]`, as
   `claudeweb` does. Prefer code that builds for both targets.
4. Add the locale entries for any new field keys.

Nothing else is required: provider creation seeds routing rules from
`routing_table()`, the Providers page lists the channel, and the credential
wizard follows `login()`.

## Tests

Channel tests live beside the code (`tests.rs` or `tests/`). Test what is
easy to get wrong: request preparation against a fixture secret, stream
decoding and usage extraction from captured frames, quota parsing, and the
consistency between `supports` and `routing_table()`. Do not test the
engine from a channel. Finish with `cargo fmt`, `cargo clippy` and
`cargo test`; a lint finding gets a code change, not an `#[allow]`.
