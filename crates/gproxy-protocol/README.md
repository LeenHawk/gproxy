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
gproxy-protocol = "2"
```

```rust
use gproxy_protocol::{
    ContentGenerationKind, Operation, OperationKey, Provider, request_target,
};

let key = OperationKey::content_generation(
    Operation::StreamGenerateContent,
    ContentGenerationKind::ClaudeMessages,
);

assert_eq!(key.kind.provider(), Provider::Claude);

// Method + path for this operation against the target provider.
let target = request_target(key, "claude-sonnet-4-5", true);
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
carry additive wire-type changes. Pin an exact version if you depend on the
enum surface staying byte-for-byte stable.

## License

Licensed under the [MIT License](LICENSE).
