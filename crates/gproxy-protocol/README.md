# gproxy-protocol

[![crates.io](https://img.shields.io/crates/v/gproxy-protocol.svg)](https://crates.io/crates/gproxy-protocol)
[![docs.rs](https://docs.rs/gproxy-protocol/badge.svg)](https://docs.rs/gproxy-protocol)

Framework-free wire models and operation metadata for OpenAI, Anthropic Claude,
Google Gemini, and AWS model APIs.

The crate is designed for proxies: request, response, and stream-event types
serialize and deserialize, open JSON fields round-trip in `rest`, and absent
optional fields remain absent. It depends only on `http`, `serde`, and
`serde_json` at runtime and builds for `wasm32`.

## Construction

Wire structs are non-exhaustive for external consumers because upstream APIs add
fields continuously. Construct them through their checked builder:

```rust
use gproxy_protocol::openai;

let request = openai::ListModelsRequest::builder()
    .rest(Default::default())
    .build()?;
# Ok::<(), gproxy_protocol::WireBuildError>(())
```

`gproxy_protocol::wire!` provides named-field syntax for code that creates many
nested wire values. The `exhaustive` feature is for GPROXY workspace builds; it
is intentionally disabled by default.

`OperationKey` combines a capability with its wire kind and rejects inconsistent
combinations. Protocol conversion lives in
[`gproxy-transform`](https://crates.io/crates/gproxy-transform).

## License

Licensed under the [MIT License](LICENSE).
