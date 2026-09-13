# gproxy-protocol

English | [简体中文](https://github.com/LeenHawk/gproxy/blob/4.0/crates/gproxy-protocol/README.zh-CN.md)

HTTP and WebSocket types for AI API gateways, SDKs, and protocol adapters in Rust.
Keep HTTP metadata together with raw or typed bodies, and represent established WebSocket
connections independently.

This is the v4 development API. It provides native models for model discovery, token counting,
content generation and streaming, context management, embeddings, reranking, files, images,
audio, video, and realtime sessions. HTTP metadata stays separate from typed bodies;
WebSocket messages and their duplex connections are modeled independently.

All declared fields and variants follow the referenced vendor API or official client schemas.
`rest` preserves unknown extensions; documented arbitrary JSON stays arbitrary only where the
source contract permits it. Body codecs, network clients, and cross-format conversion are not
implemented by this crate yet.

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
use gproxy_protocol::wire::openai::models::{GetModelRequest, GetModelResponse, Model};

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

## Typed generation payloads

Vendor types live under `wire::{claude, gemini, openai}`. The root `claude`, `gemini`, and
`openai` modules re-export the same types for compatibility.

| API | Request and buffered response | Stream payload |
|---|---|---|
| Claude Messages | `wire::claude::generate_content` | `wire::claude::stream::StreamEvent` |
| Gemini | `wire::gemini::generate_content` | `wire::gemini::stream::StreamChunk` |
| OpenAI Responses | `wire::openai::responses::{generate, response}` | `wire::openai::responses::stream::StreamEvent` |
| OpenAI Chat Completions | `wire::openai::chat::{request, response}` | `wire::openai::chat::stream::ChatCompletionChunk` |

Stream response aliases carry `ByteStream`. A caller decodes SSE or JSON-array framing and
then deserializes each JSON payload into the corresponding type. These types do not parse
network chunks, accumulate model output, or convert between APIs. Non-JSON transport markers
such as Chat Completions' `[DONE]` remain the caller's framing responsibility.

## Other API models

| Area | Modules |
|---|---|
| Context and conversations | `wire::openai::{compact, conversation, memory}` |
| Codex client endpoints | `wire::openai::{guardian, web_search}` |
| Embeddings and reranking | `wire::openai::{embeddings, rerank}`, `wire::gemini::embeddings` |
| File operations | `wire::openai::files`, `wire::claude::files`, `wire::gemini::files` |
| Image and audio operations | `wire::openai::{images, audio}` |
| Video | `wire::openai::video` (extended and native formats are distinct) |
| Responses WebSocket | `wire::openai::responses::websocket` |
| Realtime / WebRTC | `wire::openai::realtime`, `wire::gemini::live` |

Multipart forms expose typed metadata and actual `MultipartPart` file streams. Download
responses retain raw bodies. No model performs uploads, fetches result URLs, or converts
between the native and extended video formats.

`spec::OPERATION_SPECS` describes the modeled operation/dialect combinations and their wire
formats. It does not assert that a provider supports an operation or that a conversion exists.
TLS, ALPN and HTTP/2 client configuration belong to outbound client implementations.

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
