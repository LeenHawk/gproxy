use gproxy_channel_api::ChannelSupport;
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};

const fn family(operation: Operation, family: WireFamily) -> OperationKey {
    OperationKey::family(operation, family)
}

const fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

pub(super) static SUPPORTS: [ChannelSupport; 37] = [
    ChannelSupport::passthrough(family(Operation::ListModels, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::ListModels, WireFamily::Claude)),
    ChannelSupport::passthrough(family(Operation::ListModels, WireFamily::Gemini)),
    ChannelSupport::passthrough(family(Operation::GetModel, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::GetModel, WireFamily::Claude)),
    ChannelSupport::passthrough(family(Operation::GetModel, WireFamily::Gemini)),
    ChannelSupport::passthrough(family(Operation::CountTokens, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::CountTokens, WireFamily::Claude)),
    ChannelSupport::passthrough(family(Operation::CountTokens, WireFamily::Gemini)),
    ChannelSupport::passthrough(content(
        Operation::GenerateContent,
        ContentGenerationKind::OpenAiChat,
    )),
    ChannelSupport::passthrough(content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiChat,
    )),
    ChannelSupport::passthrough(content(
        Operation::GenerateContent,
        ContentGenerationKind::OpenAiResponses,
    )),
    ChannelSupport::passthrough(content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiResponses,
    )),
    ChannelSupport::passthrough(content(
        Operation::GenerateContent,
        ContentGenerationKind::ClaudeMessages,
    )),
    ChannelSupport::passthrough(content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::ClaudeMessages,
    )),
    ChannelSupport::passthrough(content(
        Operation::GenerateContent,
        ContentGenerationKind::GeminiGenerateContent,
    )),
    ChannelSupport::passthrough(content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::GeminiGenerateContent,
    )),
    ChannelSupport::passthrough(family(Operation::CreateEmbedding, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::CreateEmbedding, WireFamily::Gemini)),
    ChannelSupport::passthrough(family(Operation::Rerank, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::CreateSpeech, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::CreateTranscription, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::CreateTranslation, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::CreateImage, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::CreateImage, WireFamily::Gemini)),
    ChannelSupport::passthrough(family(Operation::EditImage, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::CreateVideo, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::RetrieveVideo, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::ListVideos, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::DeleteVideo, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::DownloadVideoContent, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::RemixVideo, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::CreateVideoCharacter, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::GetVideoCharacter, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::EditVideo, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::ExtendVideo, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::CompactContent, WireFamily::OpenAi)),
];
