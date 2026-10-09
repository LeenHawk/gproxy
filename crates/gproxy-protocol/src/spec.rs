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

const SSE: HttpBodyFormat = HttpBodyFormat::JsonStream(StreamFraming::Sse);
const JSON_ARRAY: HttpBodyFormat = HttpBodyFormat::JsonStream(StreamFraming::JsonArray);

const fn http(
    operation: Operation,
    dialect: Dialect,
    request: &'static [HttpBodyFormat],
    response: &'static [HttpBodyFormat],
) -> OperationSpec {
    OperationSpec {
        key: OperationKey { operation, dialect },
        transport: OperationTransport::Http { request, response },
    }
}

const fn websocket(operation: Operation, dialect: Dialect) -> OperationSpec {
    OperationSpec {
        key: OperationKey { operation, dialect },
        transport: OperationTransport::WebSocket,
    }
}

/// Format alternatives describe the modelled wire surfaces, not runtime
/// capabilities or default selection. Headers/query choose a format; ordering
/// here does not imply a default or infer boundaries from transport chunks.
pub const OPERATION_SPECS: &[OperationSpec] = {
    use Dialect::*;
    use HttpBodyFormat::*;
    use Operation::*;
    &[
        http(ListModels, OpenAi, &[Empty], &[Json]),
        http(ListModels, Claude, &[Empty], &[Json]),
        http(ListModels, Gemini, &[Empty], &[Json]),
        http(GetModel, OpenAi, &[Empty], &[Json]),
        http(GetModel, Claude, &[Empty], &[Json]),
        http(GetModel, Gemini, &[Empty], &[Json]),
        http(CountTokens, OpenAi, &[Json], &[Json]),
        http(CountTokens, Claude, &[Json], &[Json]),
        http(CountTokens, Gemini, &[Json], &[Json]),
        http(GenerateContent, OpenAi, &[Json], &[Json]),
        http(GenerateContent, Claude, &[Json], &[Json]),
        http(GenerateContent, Gemini, &[Json], &[Json]),
        http(GenerateContent, OpenAiChat, &[Json], &[Json]),
        http(StreamGenerateContent, OpenAi, &[Json], &[SSE]),
        http(StreamGenerateContent, Claude, &[Json], &[SSE]),
        http(StreamGenerateContent, OpenAiChat, &[Json], &[SSE]),
        http(StreamGenerateContent, Gemini, &[Json], &[SSE, JSON_ARRAY]),
        http(CreateModeration, OpenAi, &[Json], &[Json]),
        http(CreateDecision, OpenAi, &[Json], &[Json]),
        http(GuardianReview, OpenAi, &[Json], &[SSE]),
        http(GuardianClassify, OpenAi, &[Json], &[SSE]),
        http(CompactContent, OpenAi, &[Json], &[Json]),
        http(SummarizeMemory, OpenAi, &[Json], &[Json]),
        http(CreateConversation, OpenAi, &[Json], &[Json]),
        http(WebSearch, OpenAi, &[Json], &[Json]),
        http(Rerank, OpenAi, &[Json], &[Json]),
        http(CreateEmbedding, OpenAi, &[Json], &[Json]),
        http(CreateEmbedding, Gemini, &[Json], &[Json]),
        http(BatchCreateEmbedding, Gemini, &[Json], &[Json]),
        http(CreateImage, OpenAi, &[Json], &[Json, SSE]),
        http(EditImage, OpenAi, &[Json, Multipart], &[Json, SSE]),
        http(CreateSpeech, OpenAi, &[Json], &[Binary, SSE]),
        http(
            CreateTranscription,
            OpenAi,
            &[Multipart],
            &[Json, Text, SSE],
        ),
        http(CreateTranslation, OpenAi, &[Multipart], &[Json, Text]),
        http(CreateFile, OpenAi, &[Multipart], &[Json]),
        http(CreateFile, Claude, &[Multipart], &[Json]),
        http(
            CreateFile,
            Gemini,
            &[Multipart, Json, Binary],
            &[Json, Empty],
        ),
        http(ListFiles, OpenAi, &[Empty], &[Json]),
        http(ListFiles, Claude, &[Empty], &[Json]),
        http(ListFiles, Gemini, &[Empty], &[Json]),
        http(RetrieveFile, OpenAi, &[Empty], &[Json]),
        http(RetrieveFile, Claude, &[Empty], &[Json]),
        http(RetrieveFile, Gemini, &[Empty], &[Json]),
        http(RetrieveFileContent, OpenAi, &[Empty], &[Binary]),
        http(RetrieveFileContent, Claude, &[Empty], &[Binary]),
        http(RetrieveFileContent, Gemini, &[Empty], &[Binary]),
        http(DeleteFile, OpenAi, &[Empty], &[Json]),
        http(DeleteFile, Claude, &[Empty], &[Json]),
        http(DeleteFile, Gemini, &[Empty], &[Json]),
        http(CreateVideo, OpenAi, &[Json, Multipart], &[Json]),
        http(RetrieveVideo, OpenAi, &[Empty], &[Json]),
        http(ListVideos, OpenAi, &[Empty], &[Json]),
        http(DeleteVideo, OpenAi, &[Empty], &[Json]),
        http(DownloadVideoContent, OpenAi, &[Empty], &[Binary]),
        http(CreateRealtimeCall, OpenAi, &[Multipart], &[Text]),
        websocket(GenerateContent, OpenAiResponsesWebSocket),
        websocket(StreamGenerateContent, OpenAiResponsesWebSocket),
        websocket(ConnectRealtime, OpenAi),
        websocket(ConnectRealtime, Gemini),
    ]
};
