//! The exhaustive operation registry, grouped by [`OperationGroup`].
//!
//! Ingress paths are canonical per-family forms. Shared paths carry one row
//! per family and classification selects among them through the same matcher.

mod audio;
mod compact;
mod count_tokens;
mod embeddings;
mod files;
mod generate_content;
mod images;
mod memories;
mod models;
mod realtime;
mod rerank;
mod search;
mod video;

use http::Method;

use crate::operation::{Operation, OperationKind, WireFamily};
use crate::spec::{
    Affinity, Ingress, OperationSpec, PathPattern, Seg, SettleMode, StreamDetect, default_framing,
};

pub(super) const GET: &Method = &Method::GET;
pub(super) const POST: &Method = &Method::POST;
pub(super) const DELETE: &Method = &Method::DELETE;
pub(super) const FAM_OAI: OperationKind = OperationKind::Family(WireFamily::OpenAi);
pub(super) const FAM_CLA: OperationKind = OperationKind::Family(WireFamily::Claude);
pub(super) const FAM_GEM: OperationKind = OperationKind::Family(WireFamily::Gemini);
pub(super) const NEVER: StreamDetect = StreamDetect::Never;
pub(super) const BODY_STREAM: StreamDetect = StreamDetect::BodyFlag("stream");
pub(super) const BODY_OR_FORM_STREAM: StreamDetect = StreamDetect::BodyFlagOrMultipart("stream");

pub(super) const fn ing(
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
        framing: default_framing(kind, false),
        upgrade: false,
    }
}

pub(super) const fn free(ingress: &'static [Ingress]) -> OperationSpec {
    OperationSpec {
        ingress,
        settle: SettleMode::Free,
        affinity: Affinity::None,
    }
}

pub(super) const fn billed(ingress: &'static [Ingress], affinity: Affinity) -> OperationSpec {
    OperationSpec {
        ingress,
        settle: SettleMode::OnResponse,
        affinity,
    }
}

pub(super) const fn file_op(ingress: &'static [Ingress]) -> OperationSpec {
    OperationSpec {
        ingress,
        settle: SettleMode::Free,
        affinity: Affinity::Resource("file"),
    }
}

pub(super) const fn video_op(ingress: &'static [Ingress]) -> OperationSpec {
    OperationSpec {
        ingress,
        settle: SettleMode::Free,
        affinity: Affinity::Resource("video"),
    }
}

pub(super) const fn video_character_op(ingress: &'static [Ingress]) -> OperationSpec {
    OperationSpec {
        ingress,
        settle: SettleMode::Free,
        affinity: Affinity::Resource("video_character"),
    }
}

pub(crate) static REGISTRY: [(Operation, OperationSpec); 32] = [
    (Operation::ListModels, models::LIST_MODELS),
    (Operation::GetModel, models::GET_MODEL),
    (Operation::CountTokens, count_tokens::COUNT_TOKENS),
    (Operation::SummarizeMemory, memories::SUMMARIZE),
    (Operation::GenerateContent, generate_content::GENERATE),
    (
        Operation::StreamGenerateContent,
        generate_content::STREAM_GENERATE,
    ),
    (Operation::CompactContent, compact::COMPACT),
    (Operation::CreateEmbedding, embeddings::EMBEDDING),
    (Operation::BatchCreateEmbedding, embeddings::BATCH_EMBEDDING),
    (Operation::Rerank, rerank::RERANK),
    (Operation::WebSearch, search::WEB_SEARCH),
    (Operation::CreateImage, images::CREATE_IMAGE),
    (Operation::EditImage, images::EDIT_IMAGE),
    (Operation::CreateSpeech, audio::SPEECH),
    (Operation::CreateTranscription, audio::TRANSCRIBE),
    (Operation::CreateTranslation, audio::TRANSLATE),
    (Operation::CreateFile, files::CREATE_FILE),
    (Operation::ListFiles, files::LIST_FILES),
    (Operation::RetrieveFileContent, files::FILE_CONTENT),
    (Operation::RetrieveFile, files::RETRIEVE_FILE),
    (Operation::DeleteFile, files::DELETE_FILE),
    (Operation::CreateVideo, video::CREATE_VIDEO),
    (Operation::RetrieveVideo, video::RETRIEVE_VIDEO),
    (Operation::ListVideos, video::LIST_VIDEOS),
    (Operation::DeleteVideo, video::DELETE_VIDEO),
    (Operation::DownloadVideoContent, video::VIDEO_CONTENT),
    (Operation::RemixVideo, video::REMIX_VIDEO),
    (
        Operation::CreateVideoCharacter,
        video::CREATE_VIDEO_CHARACTER,
    ),
    (Operation::GetVideoCharacter, video::GET_VIDEO_CHARACTER),
    (Operation::EditVideo, video::EDIT_VIDEO),
    (Operation::ExtendVideo, video::EXTEND_VIDEO),
    (Operation::CreateRealtimeCall, realtime::REALTIME_CALL),
];

/// Exhaustive: a new operation does not compile until its group and row exist.
pub(crate) fn spec(operation: Operation) -> &'static OperationSpec {
    use Operation::*;
    let index = match operation {
        ListModels => 0,
        GetModel => 1,
        CountTokens => 2,
        SummarizeMemory => 3,
        GenerateContent => 4,
        StreamGenerateContent => 5,
        CompactContent => 6,
        CreateEmbedding => 7,
        BatchCreateEmbedding => 8,
        Rerank => 9,
        WebSearch => 10,
        CreateImage => 11,
        EditImage => 12,
        CreateSpeech => 13,
        CreateTranscription => 14,
        CreateTranslation => 15,
        CreateFile => 16,
        ListFiles => 17,
        RetrieveFileContent => 18,
        RetrieveFile => 19,
        DeleteFile => 20,
        CreateVideo => 21,
        RetrieveVideo => 22,
        ListVideos => 23,
        DeleteVideo => 24,
        DownloadVideoContent => 25,
        RemixVideo => 26,
        CreateVideoCharacter => 27,
        GetVideoCharacter => 28,
        EditVideo => 29,
        ExtendVideo => 30,
        CreateRealtimeCall => 31,
    };
    &REGISTRY[index].1
}
