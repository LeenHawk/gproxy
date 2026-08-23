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

const LIST_MODELS: OperationSpec = free(&[
    ing(GET, &[Lit("v1"), Lit("models")], FAM_OAI, NEVER),
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
        BODY_STREAM,
    )],
    Affinity::None,
);
const SPEECH: OperationSpec = billed(
    &[ing(
        POST,
        &[Lit("v1"), Lit("audio"), Lit("speech")],
        FAM_OAI,
        NEVER,
    )],
    Affinity::None,
);
const TRANSCRIBE: OperationSpec = billed(
    &[ing(
        POST,
        &[Lit("v1"), Lit("audio"), Lit("transcriptions")],
        FAM_OAI,
        NEVER,
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
const LIST_FILES: OperationSpec = free(&[ing(GET, &[Lit("v1"), Lit("files")], FAM_OAI, NEVER)]);
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
const CREATE_VIDEO: OperationSpec = OperationSpec {
    ingress: &[ing(POST, &[Lit("v1"), Lit("videos")], FAM_OAI, NEVER)],
    settle: SettleMode::Free,
    affinity: Affinity::Resource("video"),
};
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

/// Exhaustive: a new [`Operation`] does not compile until it has a row here.
pub(crate) fn spec(operation: Operation) -> &'static OperationSpec {
    use Operation::*;
    match operation {
        ListModels => &LIST_MODELS,
        GetModel => &GET_MODEL,
        CountTokens => &COUNT_TOKENS,
        GenerateContent => &GENERATE,
        StreamGenerateContent => &STREAM_GENERATE,
        CompactContent => &COMPACT,
        CreateEmbedding => &EMBEDDING,
        Rerank => &RERANK,
        WebSearch => &WEB_SEARCH,
        CreateImage => &CREATE_IMAGE,
        EditImage => &EDIT_IMAGE,
        CreateSpeech => &SPEECH,
        CreateTranscription => &TRANSCRIBE,
        CreateTranslation => &TRANSLATE,
        CreateFile => &CREATE_FILE,
        ListFiles => &LIST_FILES,
        RetrieveFile => &RETRIEVE_FILE,
        RetrieveFileContent => &FILE_CONTENT,
        DeleteFile => &DELETE_FILE,
        CreateVideo => &CREATE_VIDEO,
        RetrieveVideo => &RETRIEVE_VIDEO,
    }
}
