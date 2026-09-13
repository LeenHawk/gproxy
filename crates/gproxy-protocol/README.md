# gproxy-protocol

English | [简体中文](https://github.com/LeenHawk/gproxy/blob/4.0/crates/gproxy-protocol/README.zh-CN.md)

HTTP and WebSocket types for AI API gateways, SDKs, and protocol adapters in Rust.
Keep HTTP metadata together with raw or typed bodies, and represent established WebSocket
connections independently.

This is the v4 development API. It includes connection models, OpenAI/Claude/Gemini model
metadata and token-counting body types, including the documented native media, tool, and
configuration structures. Content generation models, body codecs, and cross-format conversion
are under development.

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

## HTTP requests and responses

`WireRequest<B>` contains `method`, `path`, `query`, `headers`, and `body`.
`WireResponse<B>` contains `status`, `headers`, and `body`.
The body type can be raw `HttpBody`, parsed multipart, or a vendor's JSON body type.

```rust
use gproxy_protocol::connection::{HeaderMap, Method, StatusCode};
use gproxy_protocol::openai::models::{GetModelRequest, GetModelResponse, Model};

let request = GetModelRequest {
    method: Method::GET,
    path: "/v1/models/gpt-5".into(),
    query: None,
    headers: HeaderMap::new(),
    body: (), // No HTTP body.
};

let body: Model = serde_json::from_str(r#"{
    "id": "gpt-5", "object": "model", "created": 0, "owned_by": "openai"
}"#).unwrap();
let response = GetModelResponse {
    status: StatusCode::OK,
    headers: HeaderMap::new(),
    body,
};

assert_eq!(request.path, "/v1/models/gpt-5");
assert_eq!(response.body.id, "gpt-5");
```

Add `serde_json = "1"` to run this example. Serialize or deserialize the JSON body itself;
HTTP method, path, query, status, and headers remain outside it. Query strings retain their
original encoding, parameter order, and duplicate keys.

Vendor `CountTokensRequest` and `CountTokensResponse` aliases wrap their respective
`CountTokensRequestBody` and `CountTokensResponseBody` types in the same HTTP envelopes.
A model name stays in the JSON body when the vendor defines it there; Gemini's outer model
resource is carried in the request path.

## Streaming, multipart, and WebSocket

- `HttpBody::Bytes` holds a complete encoded body; `HttpBody::Stream` holds raw byte chunks.
  A chunk may split a UTF-8 character, JSON value, SSE event, or multipart boundary.
- `StreamFraming` describes SSE, an incremental JSON array, or NDJSON. It is format metadata,
  not a parser or an assumption about transport chunk boundaries.
- `Multipart` holds an ordered stream of parts. Each `MultipartPart` has its own headers and
  buffered or streaming body. Repeated field names are preserved.
- WebSocket handshakes use HTTP requests and responses. The established `WebSocket` exposes
  independent incoming and outgoing streams/sinks, carrying text, binary, ping, pong, and close.
  It is not an HTTP body variant. Transport adapters handle connection setup and wire framing.

## Operations and dialects

Use `OperationKey { operation, dialect }` to identify an API operation and its wire format.
Keys implement `Copy`, `Eq`, `Hash`, and `Ord` for use in maps and sets.

| Dialect | Content generation API |
|---|---|
| `OpenAi` | OpenAI Responses |
| `Claude` | Anthropic Messages |
| `Gemini` | Gemini GenerateContent |
| `OpenAiChat` | OpenAI Chat Completions |
| `OpenAiResponsesWebSocket` | OpenAI Responses over WebSocket |

The dialect identifies the format independently of which service hosts the model.
Constructing a key does not check conversion or backend support.

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
