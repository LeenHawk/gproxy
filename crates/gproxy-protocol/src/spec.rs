//! Declared wire surfaces, independent of routing and provider capabilities.
//!
//! This table describes the protocol models available in this crate. It is not
//! a transform matrix or a list of operations every upstream must implement.
//! Error responses can use other bodies. Paths, TLS/client configuration,
//! authentication, billing and affinity belong to their respective adapters or
//! engine layers. No request is validated or sent by consulting this table.

use crate::{Dialect, Operation, OperationKey, connection::StreamFraming};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpBodyFormat {
    Empty,
    Json,
    Multipart,
    Binary,
    Text,
    /// A sequence of JSON payloads carried using HTTP stream framing.
    JsonStream(StreamFraming),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationTransport {
    Http {
        request: &'static [HttpBodyFormat],
        response: &'static [HttpBodyFormat],
    },
    /// The HTTP handshake is followed by an independent duplex connection.
    WebSocket,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationSpec {
    pub key: OperationKey,
    pub transport: OperationTransport,
}

/// Format alternatives describe the modelled wire surfaces, not runtime
/// capabilities or default selection. Headers/query choose a format; ordering
/// here does not imply a default or infer boundaries from transport chunks.
pub const OPERATION_SPECS: &[OperationSpec] = &[
    OperationSpec {
        key: OperationKey {
            operation: Operation::ListModels,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::ListModels,
            dialect: Dialect::Claude,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::ListModels,
            dialect: Dialect::Gemini,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::GetModel,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::GetModel,
            dialect: Dialect::Claude,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::GetModel,
            dialect: Dialect::Gemini,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::CountTokens,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::CountTokens,
            dialect: Dialect::Claude,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::CountTokens,
            dialect: Dialect::Gemini,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::GenerateContent,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::GenerateContent,
            dialect: Dialect::Claude,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::GenerateContent,
            dialect: Dialect::Gemini,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::GenerateContent,
            dialect: Dialect::OpenAiChat,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::StreamGenerateContent,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[HttpBodyFormat::JsonStream(StreamFraming::Sse)],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::StreamGenerateContent,
            dialect: Dialect::Claude,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[HttpBodyFormat::JsonStream(StreamFraming::Sse)],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::StreamGenerateContent,
            dialect: Dialect::OpenAiChat,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[HttpBodyFormat::JsonStream(StreamFraming::Sse)],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::StreamGenerateContent,
            dialect: Dialect::Gemini,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[
                HttpBodyFormat::JsonStream(StreamFraming::Sse),
                HttpBodyFormat::JsonStream(StreamFraming::JsonArray),
            ],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::CreateModeration,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::GuardianReview,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[HttpBodyFormat::JsonStream(StreamFraming::Sse)],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::GuardianClassify,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[HttpBodyFormat::JsonStream(StreamFraming::Sse)],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::CompactContent,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::SummarizeMemory,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::CreateConversation,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::WebSearch,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::Rerank,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::CreateEmbedding,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::CreateEmbedding,
            dialect: Dialect::Gemini,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::BatchCreateEmbedding,
            dialect: Dialect::Gemini,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::CreateImage,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[
                HttpBodyFormat::Json,
                HttpBodyFormat::JsonStream(StreamFraming::Sse),
            ],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::EditImage,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json, HttpBodyFormat::Multipart],
            response: &[
                HttpBodyFormat::Json,
                HttpBodyFormat::JsonStream(StreamFraming::Sse),
            ],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::CreateSpeech,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json],
            response: &[
                HttpBodyFormat::Binary,
                HttpBodyFormat::JsonStream(StreamFraming::Sse),
            ],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::CreateTranscription,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Multipart],
            response: &[
                HttpBodyFormat::Json,
                HttpBodyFormat::Text,
                HttpBodyFormat::JsonStream(StreamFraming::Sse),
            ],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::CreateTranslation,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Multipart],
            response: &[HttpBodyFormat::Json, HttpBodyFormat::Text],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::CreateFile,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Multipart],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::CreateFile,
            dialect: Dialect::Claude,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Multipart],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::CreateFile,
            dialect: Dialect::Gemini,
        },
        transport: OperationTransport::Http {
            request: &[
                HttpBodyFormat::Multipart,
                HttpBodyFormat::Json,
                HttpBodyFormat::Binary,
            ],
            response: &[HttpBodyFormat::Json, HttpBodyFormat::Empty],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::ListFiles,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::ListFiles,
            dialect: Dialect::Claude,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::ListFiles,
            dialect: Dialect::Gemini,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::RetrieveFile,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::RetrieveFile,
            dialect: Dialect::Claude,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::RetrieveFile,
            dialect: Dialect::Gemini,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::RetrieveFileContent,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Binary],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::RetrieveFileContent,
            dialect: Dialect::Claude,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Binary],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::RetrieveFileContent,
            dialect: Dialect::Gemini,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Binary],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::DeleteFile,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::DeleteFile,
            dialect: Dialect::Claude,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::DeleteFile,
            dialect: Dialect::Gemini,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::CreateVideo,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Json, HttpBodyFormat::Multipart],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::RetrieveVideo,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::ListVideos,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::DeleteVideo,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Json],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::DownloadVideoContent,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Empty],
            response: &[HttpBodyFormat::Binary],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::CreateRealtimeCall,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::Http {
            request: &[HttpBodyFormat::Multipart],
            response: &[HttpBodyFormat::Text],
        },
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::GenerateContent,
            dialect: Dialect::OpenAiResponsesWebSocket,
        },
        transport: OperationTransport::WebSocket,
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::StreamGenerateContent,
            dialect: Dialect::OpenAiResponsesWebSocket,
        },
        transport: OperationTransport::WebSocket,
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::ConnectRealtime,
            dialect: Dialect::OpenAi,
        },
        transport: OperationTransport::WebSocket,
    },
    OperationSpec {
        key: OperationKey {
            operation: Operation::ConnectRealtime,
            dialect: Dialect::Gemini,
        },
        transport: OperationTransport::WebSocket,
    },
];
