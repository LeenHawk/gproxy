# gproxy-protocol

[![crates.io](https://img.shields.io/crates/v/gproxy-protocol.svg)](https://crates.io/crates/gproxy-protocol)
[![docs.rs](https://docs.rs/gproxy-protocol/badge.svg)](https://docs.rs/gproxy-protocol)
[![license](https://img.shields.io/crates/l/gproxy-protocol.svg)](https://github.com/LeenHawk/gproxy)

[中文文档 / Chinese](README.zh-CN.md)

Wire-format types and cross-protocol transforms for the three major LLM APIs —
Anthropic Claude, OpenAI (ChatCompletions + Responses), and Google Gemini.
Zero HTTP dependencies, pure `serde` data + typed converters.

`gproxy-protocol` is the protocol types crate of the
[gproxy](https://github.com/LeenHawk/gproxy) project. It covers request,
response, streaming event, and shared types for the Claude, OpenAI, and Gemini
APIs, and provides cross-protocol conversions through the `transform` module.

## Quick start

Typed cross-protocol conversion via `TryFrom`:

```rust
use gproxy_protocol::claude::create_message::request::ClaudeCreateMessageRequest;
use gproxy_protocol::openai::create_chat_completions::request::OpenAiChatCompletionsRequest;

fn claude_to_openai(
    req: ClaudeCreateMessageRequest,
) -> Result<OpenAiChatCompletionsRequest, gproxy_protocol::transform::TransformError> {
    OpenAiChatCompletionsRequest::try_from(req)
}
```

Or runtime-keyed dispatch when source / destination only come from config:

```rust
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};
use gproxy_protocol::transform::dispatch::transform_request;

let out: Vec<u8> = transform_request(
    OperationFamily::GenerateContent, ProtocolKind::Claude,
    OperationFamily::GenerateContent, ProtocolKind::OpenAiChatCompletion,
    body_bytes,
)?;
```

SSE → NDJSON framing rewriter for gateway passthrough:

```rust
use gproxy_protocol::stream::SseToNdjsonRewriter;

let mut rewriter = SseToNdjsonRewriter::default();
let out = rewriter.push_chunk(b"data: {\"text\":\"hi\"}\n\n");
assert_eq!(out, b"{\"text\":\"hi\"}\n");
```

## Public Entry Points

| Entry Point | Source | Description |
| --- | --- | --- |
| `pub mod claude` | `src/lib.rs` | Claude protocol module. |
| `pub mod openai` | `src/lib.rs` | OpenAI protocol module. |
| `pub mod gemini` | `src/lib.rs` | Gemini protocol module. |
| `pub mod stream` | `src/lib.rs` | SSE / NDJSON stream processing utilities. |
| `pub mod transform` | `src/lib.rs` | Cross-protocol transformation matrix. |
| `pub use kinds::{OperationFamily, ProtocolKind}` | `src/lib.rs` | Shared operation and protocol enums used across modules. |

## Supported Protocols

| Protocol | Module | Description |
| --- | --- | --- |
| Claude | `gproxy_protocol::claude` | Types related to the Anthropic Messages API, files, models, and token counting. |
| OpenAI | `gproxy_protocol::openai` | Types for Chat Completions, Responses, Embeddings, Images, Models, and related APIs. |
| Gemini | `gproxy_protocol::gemini` | Types for GenerateContent, StreamGenerateContent, Embeddings, Batch Embeddings, Models, and the Live API. |

## Protocol Operation List

### Claude

| Operation Module | Primary Endpoint / Purpose | Typical Public Types |
| --- | --- | --- |
| `create_message` | `POST /v1/messages` | `ClaudeCreateMessageRequest`, `ClaudeCreateMessageResponse`, `ClaudeStreamEvent`, `BetaMessage` |
| `count_tokens` | `POST /v1/messages/count_tokens` | `ClaudeCountTokensRequest`, `ClaudeCountTokensResponse`, `BetaMessageTokensCount` |
| `model_list` | `GET /v1/models` | `ClaudeModelListRequest`, `ClaudeModelListResponse` |
| `model_get` | `GET /v1/models/{model_id}` | `ClaudeModelGetRequest`, `ClaudeModelGetResponse`, `BetaModelInfo` |
| `file_upload` | `POST /v1/files` | `ClaudeFileUploadRequest`, `ClaudeFileUploadResponse`, `FileMetadata` |
| `file_list` | `GET /v1/files` | `ClaudeFileListRequest`, `ClaudeFileListResponse` |
| `file_download` | `GET /v1/files/{file_id}/content` | `ClaudeFileDownloadRequest`, `ClaudeFileDownloadResponse` |
| `file_get` | `GET /v1/files/{file_id}` | `ClaudeFileGetRequest`, `ClaudeFileGetResponse`, `FileMetadata` |
| `file_delete` | `DELETE /v1/files/{file_id}` | `ClaudeFileDeleteRequest`, `ClaudeFileDeleteResponse`, `DeletedFile` |

### OpenAI

| Operation Module | Primary Endpoint / Purpose | Typical Public Types |
| --- | --- | --- |
| `create_chat_completions` | `POST /v1/chat/completions` | `OpenAiChatCompletionsRequest`, `OpenAiChatCompletionsResponse`, `ChatCompletion`, `ChatCompletionChunk` |
| `create_response` | `POST /v1/responses` | `OpenAiCreateResponseRequest`, `OpenAiCreateResponseResponse`, `ResponseBody`, `ResponseStreamEvent` |
| `create_response::websocket` | Responses WebSocket connection and messages | `OpenAiCreateResponseWebSocketConnectRequest`, `OpenAiCreateResponseWebSocketClientMessage`, `OpenAiCreateResponseWebSocketServerMessage` |
| `compact_response` | `POST /v1/responses/{id}/compact` | `OpenAiCompactRequest`, `OpenAiCompactResponse`, `CompactedResponseOutputItem` |
| `count_tokens` | `POST /v1/responses/input_tokens/count` | `OpenAiCountTokensRequest`, `OpenAiCountTokensResponse` |
| `embeddings` | `POST /v1/embeddings` | `OpenAiEmbeddingsRequest`, `OpenAiEmbeddingsResponse`, `OpenAiCreateEmbeddingResponse` |
| `create_image` | `POST /v1/images/generations` | `OpenAiCreateImageRequest`, `OpenAiCreateImageResponse`, `ImageGenerationStreamEvent` |
| `create_image_edit` | `POST /v1/images/edits` | `OpenAiCreateImageEditRequest`, `OpenAiCreateImageEditResponse`, `ImageEditStreamEvent` |
| `model_list` | `GET /v1/models` | `OpenAiModelListRequest`, `OpenAiModelListResponse`, `OpenAiModelList` |
| `model_get` | `GET /v1/models/{model}` | `OpenAiModelGetRequest`, `OpenAiModelGetResponse`, `OpenAiModel` |

### Gemini

| Operation Module | Primary Endpoint / Purpose | Typical Public Types |
| --- | --- | --- |
| `generate_content` | `POST models/{model}:generateContent` | `GeminiGenerateContentRequest`, `GeminiGenerateContentResponse`, `gemini::generate_content::response::ResponseBody` |
| `stream_generate_content` | `POST models/{model}:streamGenerateContent` | `GeminiStreamGenerateContentRequest`, `GeminiStreamGenerateContentResponse`, `GeminiNdjsonChunk`, `GeminiSseChunk` |
| `count_tokens` | `POST models/{model}:countTokens` | `GeminiCountTokensRequest`, `GeminiCountTokensResponse` |
| `embeddings` | `POST models/{model}:embedContent` | `GeminiEmbedContentRequest`, `GeminiEmbedContentResponse`, `GeminiContentEmbedding` |
| `batch_embed_contents` | `POST models/{model}:batchEmbedContents` | `GeminiBatchEmbedContentsRequest`, `GeminiBatchEmbedContentsResponse`, `BatchRequestItem` |
| `model_list` | `GET models` | `GeminiModelListRequest`, `GeminiModelListResponse` |
| `model_get` | `GET models/{model}` | `GeminiModelGetRequest`, `GeminiModelGetResponse`, `GeminiModelInfo` |
| `live` | Live API / BidiGenerateContent WebSocket | `GeminiLiveConnectRequest`, `GeminiBidiGenerateContentClientMessage`, `GeminiBidiGenerateContentServerMessage` |

## Cross-Protocol Transformation Matrix

Notes:

- In the source tree, OpenAI is split into two target or source shapes: `openai_chat_completions` and `openai_response`.
- The table below is organized by whether the corresponding submodule exists under `src/transform/`; it does not include conversions within the same protocol.

| Operation | Claude ↔ OpenAI | Claude ↔ Gemini | OpenAI ↔ Gemini | Evidence |
| --- | --- | --- | --- | --- |
| `model_list` | Bidirectional | Bidirectional | Bidirectional | `transform/{claude,openai,gemini}/model_list/*` |
| `model_get` | Bidirectional | Bidirectional | Bidirectional | `transform/{claude,openai,gemini}/model_get/*` |
| `count_tokens` | Bidirectional | Bidirectional | Bidirectional | `transform/{claude,openai,gemini}/count_tokens/*` |
| `generate_content` | Bidirectional | Bidirectional | Bidirectional | `transform/claude/generate_content/*`, `transform/openai/generate_content/*`, `transform/gemini/generate_content/*` |
| `stream_generate_content` | Bidirectional | Bidirectional | Bidirectional | `transform/claude/stream_generate_content/*`, `transform/openai/stream_generate_content/*`, `transform/gemini/stream_generate_content/*` |
| `embeddings` | Not supported | Not supported | Bidirectional | `transform/openai/embeddings/gemini`, `transform/gemini/embeddings/openai` |
| `compact` | One-way `OpenAI → Claude` | Not supported | One-way `OpenAI → Gemini` | `transform/openai/compact/{claude,gemini}` |
| `create_image` | Not supported | Not supported | One-way `OpenAI → Gemini` | `transform/openai/create_image/gemini` |
| `create_image_edit` | Not supported | Not supported | One-way `OpenAI → Gemini` | `transform/openai/create_image_edit/gemini` |
| `websocket` / `live` | Not supported | Not supported | Each side only provides its own HTTP ↔ WebSocket bridge; no Claude interop is provided | `transform/openai/websocket/*`, `transform/gemini/websocket/*` |
| `file_*` | Not supported | Not supported | Not supported | There is no file-operation directory under `transform/` |

## Key Public Types

| Category | Type | Location | Purpose |
| --- | --- | --- | --- |
| Root type | `OperationFamily` | `src/kinds.rs` | Protocol-agnostic operation family enum. |
| Root type | `ProtocolKind` | `src/kinds.rs` | Shared protocol enum used by routing, transforms, and provider dispatch. |
| Stream utility | `SseToNdjsonRewriter` | `src/stream.rs` | Incremental SSE → NDJSON rewriter. |
| Transform | `TransformError` | `src/transform/utils.rs` | Cross-protocol transform error. |
| Transform | `TransformResult<T>` | `src/transform/utils.rs` | Type alias for cross-protocol transform results. |
| Claude | `ClaudeCreateMessageRequest` | `src/claude/create_message/request.rs` | Claude message creation request. |
| Claude | `ClaudeCreateMessageResponse` | `src/claude/create_message/response.rs` | Claude message creation response enum. |
| Claude | `ClaudeStreamEvent` | `src/claude/create_message/stream.rs` | Claude streaming event. |
| Claude | `BetaMessage` | `src/claude/create_message/types.rs` | Claude message response body. |
| Claude | `ClaudeCountTokensRequest` | `src/claude/count_tokens/request.rs` | Claude token counting request. |
| Claude | `BetaMessageTokensCount` | `src/claude/count_tokens/types.rs` | Claude token counting result. |
| Claude | `BetaModelInfo` | `src/claude/types.rs` | Claude model metadata. |
| Claude | `FileMetadata` | `src/claude/types.rs` | Claude file metadata. |
| OpenAI | `OpenAiChatCompletionsRequest` | `src/openai/create_chat_completions/request.rs` | Chat Completions request. |
| OpenAI | `ChatCompletion` | `src/openai/create_chat_completions/types.rs` | Full Chat Completions response body. |
| OpenAI | `ChatCompletionChunk` | `src/openai/create_chat_completions/stream.rs` | Chat Completions streaming chunk. |
| OpenAI | `OpenAiCreateResponseRequest` | `src/openai/create_response/request.rs` | Responses API request. |
| OpenAI | `OpenAiCreateResponseResponse` | `src/openai/create_response/response.rs` | Responses API response enum. |
| OpenAI | `ResponseStreamEvent` | `src/openai/create_response/stream.rs` | Responses API streaming event. |
| OpenAI | `OpenAiCreateResponseWebSocketConnectRequest` | `src/openai/create_response/websocket/request.rs` | Responses WebSocket connection request. |
| OpenAI | `OpenAiEmbeddingsRequest` | `src/openai/embeddings/request.rs` | Embeddings request. |
| OpenAI | `OpenAiCreateEmbeddingResponse` | `src/openai/embeddings/types.rs` | Embeddings response body. |
| OpenAI | `OpenAiCreateImageRequest` | `src/openai/create_image/request.rs` | Image generation request. |
| OpenAI | `OpenAiCreateImageEditRequest` | `src/openai/create_image_edit/request.rs` | Image edit request. |
| OpenAI | `OpenAiCompactRequest` | `src/openai/compact_response/request.rs` | Response compaction request. |
| OpenAI | `OpenAiModel` | `src/openai/types.rs` | OpenAI model object. |
| OpenAI | `OpenAiModelList` | `src/openai/types.rs` | OpenAI model list body. |
| Gemini | `GeminiGenerateContentRequest` | `src/gemini/generate_content/request.rs` | GenerateContent request. |
| Gemini | `GeminiGenerateContentResponse` | `src/gemini/generate_content/response.rs` | GenerateContent response enum. |
| Gemini | `GeminiStreamGenerateContentRequest` | `src/gemini/stream_generate_content/request.rs` | StreamGenerateContent request. |
| Gemini | `GeminiNdjsonChunk` | `src/gemini/stream_generate_content/stream.rs` | NDJSON streaming chunk. |
| Gemini | `GeminiSseChunk` | `src/gemini/stream_generate_content/stream.rs` | SSE streaming chunk. |
| Gemini | `GeminiCountTokensRequest` | `src/gemini/count_tokens/request.rs` | CountTokens request. |
| Gemini | `GeminiEmbedContentRequest` | `src/gemini/embeddings/request.rs` | Single embedding request. |
| Gemini | `GeminiBatchEmbedContentsRequest` | `src/gemini/batch_embed_contents/request.rs` | Batch embedding request. |
| Gemini | `GeminiLiveConnectRequest` | `src/gemini/live/request.rs` | Live API connection request. |
| Gemini | `GeminiBidiGenerateContentClientMessage` | `src/gemini/live/types.rs` | Live API client message. |
| Gemini | `GeminiBidiGenerateContentServerMessage` | `src/gemini/live/types.rs` | Live API server message. |
| Gemini | `GeminiModelInfo` | `src/gemini/types.rs` | Gemini model metadata. |

## License

Licensed under the [MIT License](LICENSE).
