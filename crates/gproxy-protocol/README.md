# gproxy-protocol

[![crates.io](https://img.shields.io/crates/v/gproxy-protocol.svg)](https://crates.io/crates/gproxy-protocol)
[![docs.rs](https://docs.rs/gproxy-protocol/badge.svg)](https://docs.rs/gproxy-protocol)
[![license](https://img.shields.io/crates/l/gproxy-protocol.svg)](LICENSE)

Wire-format types and endpoint metadata for the OpenAI, Anthropic Claude, and
Google Gemini APIs.

`gproxy-protocol` is the protocol layer used by
[GPROXY](https://github.com/LeenHawk/gproxy). It provides serializable request,
response, streaming-event, and shared operation types without coupling them to
an HTTP client or server implementation. Dependencies are `serde`, `serde_json`,
and `http` — nothing else, and it builds for `wasm32`.

## Usage

```toml
[dependencies]
gproxy-protocol = "=2.3.0"
```

```rust
use gproxy_protocol::{
    ContentGenerationKind, Operation, OperationKey, Provider, request_target,
};

let key = OperationKey::content_generation(
    Operation::StreamGenerateContent,
    ContentGenerationKind::ClaudeMessages,
);

assert_eq!(key.kind().provider(), Provider::Claude);

// Method + path for this operation against the target provider.
let target = request_target(key, "claude-sonnet-4-5", true)?;
```

## Layout

- `gproxy_protocol::openai` — chat completions, responses, embeddings, images,
  models, conversations, compaction.
- `gproxy_protocol::claude` — messages, count tokens, models.
- `gproxy_protocol::gemini` — generateContent, embeddings, caching, batch,
  count tokens, models.
- Root re-exports — the shared taxonomy: `Provider`, `Operation`,
  `OperationGroup`, `OperationKind`, `ContentGenerationKind`, `OperationKey`,
  plus `Endpoint` / `HttpMethod` / `request_target` endpoint metadata.

These modules model provider wire shapes **only**. Conversion between them lives
in [`gproxy-transform`](https://crates.io/crates/gproxy-transform).

## Compatibility

The crate version tracks the GPROXY release it ships with, so a minor bump can
carry additive wire-type changes. Public wire structs and enums are
`#[non_exhaustive]`: construct named structs through their generated
`Type::builder()` or the `gproxy_protocol::wire!` helper, and include a wildcard
arm when matching enums. Lockstep GPROXY crates use exact dependency versions;
downstreams that combine protocol and transform should pin the same exact
version as well.

`OperationKey` protects the operation/kind invariant. Its fields are private;
read them with `operation()` / `kind()` and construct keys with
`content_generation`, `provider`, or checked `try_new`.

`request_target` accepts a raw model id, encodes it as one path/query component,
and returns a structured error for inconsistent or unsupported combinations.

## License

Licensed under the [MIT License](LICENSE).
