# gproxy-protocol

`gproxy-protocol` 是 gproxy SDK 的协议类型库。它覆盖 Claude、OpenAI 和 Gemini 三套 API 的请求、响应、流式事件与共享类型，并通过 `transform` 模块提供跨协议转换。

## 公开入口

| 入口 | 来源 | 说明 |
| --- | --- | --- |
| `pub mod claude` | `src/lib.rs` | Claude 协议模块。 |
| `pub mod openai` | `src/lib.rs` | OpenAI 协议模块。 |
| `pub mod gemini` | `src/lib.rs` | Gemini 协议模块。 |
| `pub mod stream` | `src/lib.rs` | SSE / NDJSON 流处理工具。 |
| `pub mod transform` | `src/lib.rs` | 跨协议转换矩阵。 |
| `pub use kinds::{OperationFamily, ProtocolKind}` | `src/lib.rs` | 跨模块共享的操作与协议枚举。 |

## 支持的协议

| 协议 | 模块 | 说明 |
| --- | --- | --- |
| Claude | `gproxy_protocol::claude` | Anthropic Messages API 与文件、模型、计数相关类型。 |
| OpenAI | `gproxy_protocol::openai` | Chat Completions、Responses、Embeddings、Images、Models 等类型。 |
| Gemini | `gproxy_protocol::gemini` | GenerateContent、StreamGenerateContent、Embeddings、Batch Embeddings、Models、Live API 类型。 |

## 协议操作列表

### Claude

| 操作模块 | 主要 endpoint / 用途 | 典型公开类型 |
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

| 操作模块 | 主要 endpoint / 用途 | 典型公开类型 |
| --- | --- | --- |
| `create_chat_completions` | `POST /v1/chat/completions` | `OpenAiChatCompletionsRequest`, `OpenAiChatCompletionsResponse`, `ChatCompletion`, `ChatCompletionChunk` |
| `create_response` | `POST /v1/responses` | `OpenAiCreateResponseRequest`, `OpenAiCreateResponseResponse`, `ResponseBody`, `ResponseStreamEvent` |
| `create_response::websocket` | Responses WebSocket 连接与消息 | `OpenAiCreateResponseWebSocketConnectRequest`, `OpenAiCreateResponseWebSocketClientMessage`, `OpenAiCreateResponseWebSocketServerMessage` |
| `compact_response` | `POST /v1/responses/{id}/compact` | `OpenAiCompactRequest`, `OpenAiCompactResponse`, `CompactedResponseOutputItem` |
| `count_tokens` | `POST /v1/responses/input_tokens/count` | `OpenAiCountTokensRequest`, `OpenAiCountTokensResponse` |
| `embeddings` | `POST /v1/embeddings` | `OpenAiEmbeddingsRequest`, `OpenAiEmbeddingsResponse`, `OpenAiCreateEmbeddingResponse` |
| `create_image` | `POST /v1/images/generations` | `OpenAiCreateImageRequest`, `OpenAiCreateImageResponse`, `ImageGenerationStreamEvent` |
| `create_image_edit` | `POST /v1/images/edits` | `OpenAiCreateImageEditRequest`, `OpenAiCreateImageEditResponse`, `ImageEditStreamEvent` |
| `model_list` | `GET /v1/models` | `OpenAiModelListRequest`, `OpenAiModelListResponse`, `OpenAiModelList` |
| `model_get` | `GET /v1/models/{model}` | `OpenAiModelGetRequest`, `OpenAiModelGetResponse`, `OpenAiModel` |

### Gemini

| 操作模块 | 主要 endpoint / 用途 | 典型公开类型 |
| --- | --- | --- |
| `generate_content` | `POST models/{model}:generateContent` | `GeminiGenerateContentRequest`, `GeminiGenerateContentResponse`, `gemini::generate_content::response::ResponseBody` |
| `stream_generate_content` | `POST models/{model}:streamGenerateContent` | `GeminiStreamGenerateContentRequest`, `GeminiStreamGenerateContentResponse`, `GeminiNdjsonChunk`, `GeminiSseChunk` |
| `count_tokens` | `POST models/{model}:countTokens` | `GeminiCountTokensRequest`, `GeminiCountTokensResponse` |
| `embeddings` | `POST models/{model}:embedContent` | `GeminiEmbedContentRequest`, `GeminiEmbedContentResponse`, `GeminiContentEmbedding` |
| `batch_embed_contents` | `POST models/{model}:batchEmbedContents` | `GeminiBatchEmbedContentsRequest`, `GeminiBatchEmbedContentsResponse`, `BatchRequestItem` |
| `model_list` | `GET models` | `GeminiModelListRequest`, `GeminiModelListResponse` |
| `model_get` | `GET models/{model}` | `GeminiModelGetRequest`, `GeminiModelGetResponse`, `GeminiModelInfo` |
| `live` | Live API / BidiGenerateContent WebSocket | `GeminiLiveConnectRequest`, `GeminiBidiGenerateContentClientMessage`, `GeminiBidiGenerateContentServerMessage` |

## 跨协议转换矩阵

说明：

- OpenAI 在源码里分成 `openai_chat_completions` 和 `openai_response` 两种目标或来源形态。
- 下表依据 `src/transform/` 目录是否存在对应子模块整理，不包含同协议内部转换。

| 操作 | Claude ↔ OpenAI | Claude ↔ Gemini | OpenAI ↔ Gemini | 依据 |
| --- | --- | --- | --- | --- |
| `model_list` | 双向 | 双向 | 双向 | `transform/{claude,openai,gemini}/model_list/*` |
| `model_get` | 双向 | 双向 | 双向 | `transform/{claude,openai,gemini}/model_get/*` |
| `count_tokens` | 双向 | 双向 | 双向 | `transform/{claude,openai,gemini}/count_tokens/*` |
| `generate_content` | 双向 | 双向 | 双向 | `transform/claude/generate_content/*`, `transform/openai/generate_content/*`, `transform/gemini/generate_content/*` |
| `stream_generate_content` | 双向 | 双向 | 双向 | `transform/claude/stream_generate_content/*`, `transform/openai/stream_generate_content/*`, `transform/gemini/stream_generate_content/*` |
| `embeddings` | 不支持 | 不支持 | 双向 | `transform/openai/embeddings/gemini`, `transform/gemini/embeddings/openai` |
| `compact` | 单向 `OpenAI → Claude` | 不支持 | 单向 `OpenAI → Gemini` | `transform/openai/compact/{claude,gemini}` |
| `create_image` | 不支持 | 不支持 | 单向 `OpenAI → Gemini` | `transform/openai/create_image/gemini` |
| `create_image_edit` | 不支持 | 不支持 | 单向 `OpenAI → Gemini` | `transform/openai/create_image_edit/gemini` |
| `websocket` / `live` | 不支持 | 不支持 | 仅各自提供 HTTP ↔ WebSocket bridge，不提供 Claude 互转 | `transform/openai/websocket/*`, `transform/gemini/websocket/*` |
| `file_*` | 不支持 | 不支持 | 不支持 | `transform/` 下没有文件操作目录 |

## 关键公开类型

| 分类 | 类型 | 位置 | 用途 |
| --- | --- | --- | --- |
| 根类型 | `OperationFamily` | `src/kinds.rs` | 协议无关的操作族枚举。 |
| 根类型 | `ProtocolKind` | `src/kinds.rs` | 路由、转换、provider 分发共用的协议枚举。 |
| 流工具 | `SseToNdjsonRewriter` | `src/stream.rs` | 增量式 SSE → NDJSON 重写器。 |
| 转换 | `TransformError` | `src/transform/utils.rs` | 跨协议转换错误。 |
| 转换 | `TransformResult<T>` | `src/transform/utils.rs` | 跨协议转换结果别名。 |
| Claude | `ClaudeCreateMessageRequest` | `src/claude/create_message/request.rs` | Claude 消息创建请求。 |
| Claude | `ClaudeCreateMessageResponse` | `src/claude/create_message/response.rs` | Claude 消息创建响应枚举。 |
| Claude | `ClaudeStreamEvent` | `src/claude/create_message/stream.rs` | Claude 流式事件。 |
| Claude | `BetaMessage` | `src/claude/create_message/types.rs` | Claude 消息响应主体。 |
| Claude | `ClaudeCountTokensRequest` | `src/claude/count_tokens/request.rs` | Claude token 计数请求。 |
| Claude | `BetaMessageTokensCount` | `src/claude/count_tokens/types.rs` | Claude token 计数结果。 |
| Claude | `BetaModelInfo` | `src/claude/types.rs` | Claude 模型元信息。 |
| Claude | `FileMetadata` | `src/claude/types.rs` | Claude 文件元数据。 |
| OpenAI | `OpenAiChatCompletionsRequest` | `src/openai/create_chat_completions/request.rs` | Chat Completions 请求。 |
| OpenAI | `ChatCompletion` | `src/openai/create_chat_completions/types.rs` | Chat Completions 完整响应主体。 |
| OpenAI | `ChatCompletionChunk` | `src/openai/create_chat_completions/stream.rs` | Chat Completions 流 chunk。 |
| OpenAI | `OpenAiCreateResponseRequest` | `src/openai/create_response/request.rs` | Responses API 请求。 |
| OpenAI | `OpenAiCreateResponseResponse` | `src/openai/create_response/response.rs` | Responses API 响应枚举。 |
| OpenAI | `ResponseStreamEvent` | `src/openai/create_response/stream.rs` | Responses API 流式事件。 |
| OpenAI | `OpenAiCreateResponseWebSocketConnectRequest` | `src/openai/create_response/websocket/request.rs` | Responses WebSocket 连接请求。 |
| OpenAI | `OpenAiEmbeddingsRequest` | `src/openai/embeddings/request.rs` | Embeddings 请求。 |
| OpenAI | `OpenAiCreateEmbeddingResponse` | `src/openai/embeddings/types.rs` | Embeddings 响应主体。 |
| OpenAI | `OpenAiCreateImageRequest` | `src/openai/create_image/request.rs` | 图片生成请求。 |
| OpenAI | `OpenAiCreateImageEditRequest` | `src/openai/create_image_edit/request.rs` | 图片编辑请求。 |
| OpenAI | `OpenAiCompactRequest` | `src/openai/compact_response/request.rs` | 响应压缩请求。 |
| OpenAI | `OpenAiModel` | `src/openai/types.rs` | OpenAI 模型对象。 |
| OpenAI | `OpenAiModelList` | `src/openai/types.rs` | OpenAI 模型列表主体。 |
| Gemini | `GeminiGenerateContentRequest` | `src/gemini/generate_content/request.rs` | GenerateContent 请求。 |
| Gemini | `GeminiGenerateContentResponse` | `src/gemini/generate_content/response.rs` | GenerateContent 响应枚举。 |
| Gemini | `GeminiStreamGenerateContentRequest` | `src/gemini/stream_generate_content/request.rs` | StreamGenerateContent 请求。 |
| Gemini | `GeminiNdjsonChunk` | `src/gemini/stream_generate_content/stream.rs` | NDJSON 流 chunk。 |
| Gemini | `GeminiSseChunk` | `src/gemini/stream_generate_content/stream.rs` | SSE 流 chunk。 |
| Gemini | `GeminiCountTokensRequest` | `src/gemini/count_tokens/request.rs` | CountTokens 请求。 |
| Gemini | `GeminiEmbedContentRequest` | `src/gemini/embeddings/request.rs` | 单条 embedding 请求。 |
| Gemini | `GeminiBatchEmbedContentsRequest` | `src/gemini/batch_embed_contents/request.rs` | 批量 embedding 请求。 |
| Gemini | `GeminiLiveConnectRequest` | `src/gemini/live/request.rs` | Live API 连接请求。 |
| Gemini | `GeminiBidiGenerateContentClientMessage` | `src/gemini/live/types.rs` | Live API 客户端消息。 |
| Gemini | `GeminiBidiGenerateContentServerMessage` | `src/gemini/live/types.rs` | Live API 服务端消息。 |
| Gemini | `GeminiModelInfo` | `src/gemini/types.rs` | Gemini 模型元信息。 |
