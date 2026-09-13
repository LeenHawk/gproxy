# gproxy-protocol

English | [简体中文](https://github.com/LeenHawk/gproxy/blob/4.0/crates/gproxy-protocol/README.zh-CN.md)

Protocol types for working with OpenAI, Anthropic Claude, and Google Gemini APIs in Rust.
Use them to identify API operations and wire formats in gateways, SDKs, and protocol adapters.

> This README describes the v4 development API. It currently provides operation and dialect
> identifiers. Request/response models and format conversion are under development.

## Installation

The Cargo dependency for the v4 prerelease is:

```toml
[dependencies]
gproxy-protocol = "4.0.0-dev"
```

Until that version is published, use a local checkout of the repository's `4.0` branch:

```toml
[dependencies]
gproxy-protocol = { path = "../gproxy/crates/gproxy-protocol" }
```

## Usage

An `OperationKey` combines an operation with the format used to express it:

```rust
use gproxy_protocol::{Dialect, Operation, OperationKey};

let messages = OperationKey {
    operation: Operation::GenerateContent,
    dialect: Dialect::Claude,
};

let responses = OperationKey {
    operation: Operation::GenerateContent,
    dialect: Dialect::OpenAi,
};

assert_eq!(messages.operation, responses.operation);
assert_ne!(messages.dialect, responses.dialect);
```

Keys implement `Copy`, `Eq`, `Hash`, and `Ord`, so they can be used directly in maps and sets.
Constructing a key identifies a combination; it does not check whether your application supports it.

| Dialect | Content generation API |
|---|---|
| `OpenAi` | OpenAI Responses |
| `Claude` | Anthropic Messages |
| `Gemini` | Gemini GenerateContent |
| `OpenAiChat` | OpenAI Chat Completions |
| `OpenAiResponsesWebSocket` | OpenAI Responses over WebSocket |

`Operation` also covers model listing, token counting, embeddings, images, audio, video, files,
and realtime sessions. Use `StreamGenerateContent` for streaming content generation.
The dialect identifies the wire format, independently of which service hosts the model.

## String identifiers

`Operation` and `Dialect` support parsing and stable string identifiers. Their Serde
representations use the same strings:

```rust
use gproxy_protocol::{Dialect, Operation};

let operation: Operation = "generate_content".parse().unwrap();
let dialect: Dialect = "openai_chat".parse().unwrap();

assert_eq!(operation, Operation::GenerateContent);
assert_eq!(dialect, Dialect::OpenAiChat);
assert_eq!(dialect.id(), "openai_chat");
assert!("unknown".parse::<Dialect>().is_err());
```

## Features

No features are enabled by default. Public enums are `non_exhaustive`; include a wildcard arm
when matching them. The `exhaustive` feature removes that restriction, so adding enum variants
may require updates to your matches when upgrading.

## License

[AGPL-3.0-or-later](https://www.gnu.org/licenses/agpl-3.0.html).
