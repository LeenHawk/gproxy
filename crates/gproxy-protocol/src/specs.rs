//! The registry table: one `OperationSpec` per operation, exhaustively.
//!
//! Ingress paths are the canonical per-family forms. When one path serves
//! two wire families (Claude and OpenAI both use `/v1/files`), the table
//! lists the primary family; classification disambiguates by auth profile
//! and headers — that logic lives in the engine, the facts live here.

use http::Method;

use crate::operation::ContentGenerationKind::*;
use crate::operation::Operation;
use crate::operation::OperationKind::{self, ContentGeneration as CG};
use crate::operation::WireFamily::*;
use crate::spec::Seg::{Lit, Param, ParamAction};
use crate::spec::{Affinity, Ingress, OperationSpec, PathPattern, Seg, SettleMode, StreamDetect};

const fn ing(
    method: &'static Method,
    pattern: &'static [Seg],
    kind: OperationKind,
    stream: StreamDetect,
) -> Ingress {
    Ingress {
        method,
        pattern: PathPattern(pattern),
        kind,
        stream,
        // WS-upgrade ingresses (realtime sessions, Responses-over-WS) use
        // a dedicated constructor when those operations land.
        upgrade: false,
    }
}

const GET: &Method = &Method::GET;
const POST: &Method = &Method::POST;
const DELETE: &Method = &Method::DELETE;
const FAM_OAI: OperationKind = OperationKind::Family(OpenAi);
const FAM_CLA: OperationKind = OperationKind::Family(Claude);
const FAM_GEM: OperationKind = OperationKind::Family(Gemini);
const NEVER: StreamDetect = StreamDetect::Never;
const BODY_STREAM: StreamDetect = StreamDetect::BodyFlag("stream");
const BODY_OR_FORM_STREAM: StreamDetect = StreamDetect::BodyFlagOrMultipart("stream");

const fn free(ingress: &'static [Ingress]) -> OperationSpec {
    OperationSpec {
        ingress,
        settle: SettleMode::Free,
        affinity: Affinity::None,
    }
}

const fn billed(ingress: &'static [Ingress], affinity: Affinity) -> OperationSpec {
    OperationSpec {
        ingress,
        settle: SettleMode::OnResponse,
        affinity,
    }
}

const fn file_op(ingress: &'static [Ingress]) -> OperationSpec {
    OperationSpec {
        ingress,
        settle: SettleMode::Free,
        affinity: Affinity::Resource("file"),
    }
}

const fn video_op(ingress: &'static [Ingress]) -> OperationSpec {
    OperationSpec {
        ingress,
        settle: SettleMode::Free,
        affinity: Affinity::Resource("video"),
    }
}

const fn video_character_op(ingress: &'static [Ingress]) -> OperationSpec {
    OperationSpec {
        ingress,
        settle: SettleMode::Free,
        affinity: Affinity::Resource("video_character"),
    }
}

const LIST_MODELS: OperationSpec = free(&[
    ing(GET, &[Lit("v1"), Lit("models")], FAM_OAI, NEVER),
    ing(GET, &[Lit("v1"), Lit("models")], FAM_CLA, NEVER),
    ing(GET, &[Lit("v1beta"), Lit("models")], FAM_GEM, NEVER),
]);
const GET_MODEL: OperationSpec = free(&[
    ing(
        GET,
        &[Lit("v1"), Lit("models"), Param("id")],
        FAM_OAI,
        NEVER,
    ),
    ing(
        GET,
        &[Lit("v1"), Lit("models"), Param("id")],
        FAM_CLA,
        NEVER,
    ),
    ing(
        GET,
        &[Lit("v1beta"), Lit("models"), Param("model")],
        FAM_GEM,
        NEVER,
    ),
]);
const COUNT_TOKENS: OperationSpec = free(&[
    ing(
        POST,
        &[Lit("v1"), Lit("messages"), Lit("count_tokens")],
        FAM_CLA,
        NEVER,
    ),
    ing(
        POST,
        &[Lit("v1"), Lit("responses"), Lit("input_tokens")],
        FAM_OAI,
        NEVER,
    ),
    ing(
        POST,
        &[
            Lit("v1beta"),
            Lit("models"),
            ParamAction("model", "countTokens"),
        ],
        FAM_GEM,
        NEVER,
    ),
]);
const GENERATE: OperationSpec = billed(
    &[
        ing(
            POST,
            &[Lit("v1"), Lit("chat"), Lit("completions")],
            CG(OpenAiChat),
            BODY_STREAM,
        ),
        ing(
            POST,
            &[Lit("v1"), Lit("responses")],
            CG(OpenAiResponses),
            BODY_STREAM,
        ),
        ing(
            POST,
            &[Lit("v1"), Lit("messages")],
            CG(ClaudeMessages),
            BODY_STREAM,
        ),
        ing(
            POST,
            &[
                Lit("v1beta"),
                Lit("models"),
                ParamAction("model", "generateContent"),
            ],
            CG(GeminiGenerateContent),
            NEVER,
        ),
    ],
    Affinity::Session,
);
const STREAM_GENERATE: OperationSpec = billed(
    &[ing(
        POST,
        &[
            Lit("v1beta"),
            Lit("models"),
            ParamAction("model", "streamGenerateContent"),
        ],
        CG(GeminiGenerateContent),
        StreamDetect::Always,
    )],
    Affinity::Session,
);
const COMPACT: OperationSpec = billed(
    &[ing(
        POST,
        &[Lit("v1"), Lit("responses"), Lit("compact")],
        FAM_OAI,
        NEVER,
    )],
    Affinity::None,
);
const EMBEDDING: OperationSpec = billed(
    &[ing(POST, &[Lit("v1"), Lit("embeddings")], FAM_OAI, NEVER)],
    Affinity::None,
);
const RERANK: OperationSpec = billed(
    &[ing(POST, &[Lit("v1"), Lit("rerank")], FAM_OAI, NEVER)],
    Affinity::None,
);
const WEB_SEARCH: OperationSpec = billed(
    &[ing(
        POST,
        &[Lit("v1"), Lit("alpha"), Lit("search")],
        FAM_OAI,
        NEVER,
    )],
    Affinity::None,
);
const CREATE_IMAGE: OperationSpec = billed(
    &[ing(
        POST,
        &[Lit("v1"), Lit("images"), Lit("generations")],
        FAM_OAI,
        BODY_STREAM,
    )],
    Affinity::None,
);
const EDIT_IMAGE: OperationSpec = billed(
    &[ing(
        POST,
        &[Lit("v1"), Lit("images"), Lit("edits")],
        FAM_OAI,
        BODY_OR_FORM_STREAM,
    )],
    Affinity::None,
);
const SPEECH: OperationSpec = billed(
    &[ing(
        POST,
        &[Lit("v1"), Lit("audio"), Lit("speech")],
        FAM_OAI,
        StreamDetect::BodyValue("stream_format", "sse"),
    )],
    Affinity::None,
);
const TRANSCRIBE: OperationSpec = billed(
    &[ing(
        POST,
        &[Lit("v1"), Lit("audio"), Lit("transcriptions")],
        FAM_OAI,
        BODY_OR_FORM_STREAM,
    )],
    Affinity::None,
);
const TRANSLATE: OperationSpec = billed(
    &[ing(
        POST,
        &[Lit("v1"), Lit("audio"), Lit("translations")],
        FAM_OAI,
        NEVER,
    )],
    Affinity::None,
);
const CREATE_FILE: OperationSpec =
    file_op(&[ing(POST, &[Lit("v1"), Lit("files")], FAM_OAI, NEVER)]);
const LIST_FILES: OperationSpec = file_op(&[ing(GET, &[Lit("v1"), Lit("files")], FAM_OAI, NEVER)]);
const RETRIEVE_FILE: OperationSpec = file_op(&[ing(
    GET,
    &[Lit("v1"), Lit("files"), Param("id")],
    FAM_OAI,
    NEVER,
)]);
const FILE_CONTENT: OperationSpec = file_op(&[ing(
    GET,
    &[Lit("v1"), Lit("files"), Param("id"), Lit("content")],
    FAM_OAI,
    NEVER,
)]);
const DELETE_FILE: OperationSpec = file_op(&[ing(
    DELETE,
    &[Lit("v1"), Lit("files"), Param("id")],
    FAM_OAI,
    NEVER,
)]);
const CREATE_VIDEO: OperationSpec =
    video_op(&[ing(POST, &[Lit("v1"), Lit("videos")], FAM_OAI, NEVER)]);
const RETRIEVE_VIDEO: OperationSpec = OperationSpec {
    ingress: &[ing(
        GET,
        &[Lit("v1"), Lit("videos"), Param("id")],
        FAM_OAI,
        NEVER,
    )],
    settle: SettleMode::OnCompletedStatus,
    affinity: Affinity::Resource("video"),
};
const LIST_VIDEOS: OperationSpec =
    video_op(&[ing(GET, &[Lit("v1"), Lit("videos")], FAM_OAI, NEVER)]);
const DELETE_VIDEO: OperationSpec = video_op(&[ing(
    DELETE,
    &[Lit("v1"), Lit("videos"), Param("id")],
    FAM_OAI,
    NEVER,
)]);
const VIDEO_CONTENT: OperationSpec = video_op(&[ing(
    GET,
    &[Lit("v1"), Lit("videos"), Param("id"), Lit("content")],
    FAM_OAI,
    NEVER,
)]);
const REMIX_VIDEO: OperationSpec = video_op(&[ing(
    POST,
    &[Lit("v1"), Lit("videos"), Param("id"), Lit("remix")],
    FAM_OAI,
    NEVER,
)]);
const CREATE_VIDEO_CHARACTER: OperationSpec = video_character_op(&[ing(
    POST,
    &[Lit("v1"), Lit("videos"), Lit("characters")],
    FAM_OAI,
    NEVER,
)]);
const GET_VIDEO_CHARACTER: OperationSpec = video_character_op(&[ing(
    GET,
    &[Lit("v1"), Lit("videos"), Lit("characters"), Param("id")],
    FAM_OAI,
    NEVER,
)]);
const EDIT_VIDEO: OperationSpec = video_op(&[ing(
    POST,
    &[Lit("v1"), Lit("videos"), Lit("edits")],
    FAM_OAI,
    NEVER,
)]);
const EXTEND_VIDEO: OperationSpec = video_op(&[ing(
    POST,
    &[Lit("v1"), Lit("videos"), Lit("extensions")],
    FAM_OAI,
    NEVER,
)]);

// The SDP answer carries no usage; realtime usage arrives on the session's
// event stream (proxied WS) — or not at all when WebRTC media bypasses the
// proxy. Settling Free here is honest, not an oversight; the session-side
// metering design lands with the round-3 websocket-ingress work.
const REALTIME_CALL: OperationSpec = OperationSpec {
    ingress: &[ing(
        POST,
        &[Lit("v1"), Lit("realtime"), Lit("calls")],
        FAM_OAI,
        NEVER,
    )],
    settle: SettleMode::Free,
    affinity: Affinity::Resource("realtime_call"),
};

pub(crate) static REGISTRY: [(Operation, OperationSpec); 30] = [
    (Operation::ListModels, LIST_MODELS),
    (Operation::GetModel, GET_MODEL),
    (Operation::CountTokens, COUNT_TOKENS),
    (Operation::GenerateContent, GENERATE),
    (Operation::StreamGenerateContent, STREAM_GENERATE),
    (Operation::CompactContent, COMPACT),
    (Operation::CreateEmbedding, EMBEDDING),
    (Operation::Rerank, RERANK),
    (Operation::WebSearch, WEB_SEARCH),
    (Operation::CreateImage, CREATE_IMAGE),
    (Operation::EditImage, EDIT_IMAGE),
    (Operation::CreateSpeech, SPEECH),
    (Operation::CreateTranscription, TRANSCRIBE),
    (Operation::CreateTranslation, TRANSLATE),
    (Operation::CreateFile, CREATE_FILE),
    (Operation::ListFiles, LIST_FILES),
    (Operation::RetrieveFile, RETRIEVE_FILE),
    (Operation::RetrieveFileContent, FILE_CONTENT),
    (Operation::DeleteFile, DELETE_FILE),
    (Operation::CreateVideo, CREATE_VIDEO),
    (Operation::RetrieveVideo, RETRIEVE_VIDEO),
    (Operation::ListVideos, LIST_VIDEOS),
    (Operation::DeleteVideo, DELETE_VIDEO),
    (Operation::DownloadVideoContent, VIDEO_CONTENT),
    (Operation::RemixVideo, REMIX_VIDEO),
    (Operation::CreateVideoCharacter, CREATE_VIDEO_CHARACTER),
    (Operation::GetVideoCharacter, GET_VIDEO_CHARACTER),
    (Operation::EditVideo, EDIT_VIDEO),
    (Operation::ExtendVideo, EXTEND_VIDEO),
    (Operation::CreateRealtimeCall, REALTIME_CALL),
];

/// Exhaustive: a new [`Operation`] does not compile until it has a row here.
pub(crate) fn spec(operation: Operation) -> &'static OperationSpec {
    use Operation::*;
    let index = match operation {
        ListModels => 0,
        GetModel => 1,
        CountTokens => 2,
        GenerateContent => 3,
        StreamGenerateContent => 4,
        CompactContent => 5,
        CreateEmbedding => 6,
        Rerank => 7,
        WebSearch => 8,
        CreateImage => 9,
        EditImage => 10,
        CreateSpeech => 11,
        CreateTranscription => 12,
        CreateTranslation => 13,
        CreateFile => 14,
        ListFiles => 15,
        RetrieveFile => 16,
        RetrieveFileContent => 17,
        DeleteFile => 18,
        CreateVideo => 19,
        RetrieveVideo => 20,
        ListVideos => 21,
        DeleteVideo => 22,
        DownloadVideoContent => 23,
        RemixVideo => 24,
        CreateVideoCharacter => 25,
        GetVideoCharacter => 26,
        EditVideo => 27,
        ExtendVideo => 28,
        CreateRealtimeCall => 29,
    };
    &REGISTRY[index].1
}
