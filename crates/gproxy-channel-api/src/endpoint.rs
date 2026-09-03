use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind, WireFamily};

pub fn endpoint_override_key(key: OperationKey) -> Option<&'static str> {
    if let OperationKind::ContentGeneration(kind) = key.kind() {
        return match kind {
            ContentGenerationKind::OpenAiChat => Some("openai_chat_completions"),
            ContentGenerationKind::OpenAiResponses => Some("openai_responses"),
            ContentGenerationKind::ClaudeMessages => Some("claude_messages"),
            ContentGenerationKind::GeminiGenerateContent => {
                if key.operation() == Operation::StreamGenerateContent {
                    Some("gemini_stream_generate_content")
                } else {
                    Some("gemini_generate_content")
                }
            }
            ContentGenerationKind::OpenAiResponsesWebSocket => Some("openai_responses_websocket"),
        };
    }
    use Operation::*;
    Some(match (key.operation(), key.kind()) {
        (ListModels, OperationKind::Family(WireFamily::OpenAi)) => "openai_list_models",
        (ListModels, OperationKind::Family(WireFamily::Claude)) => "claude_list_models",
        (ListModels, OperationKind::Family(WireFamily::Gemini)) => "gemini_list_models",
        (GetModel, OperationKind::Family(WireFamily::OpenAi)) => "openai_get_model",
        (GetModel, OperationKind::Family(WireFamily::Claude)) => "claude_get_model",
        (GetModel, OperationKind::Family(WireFamily::Gemini)) => "gemini_get_model",
        (CountTokens, OperationKind::Family(WireFamily::OpenAi)) => "openai_count_tokens",
        (CountTokens, OperationKind::Family(WireFamily::Claude)) => "claude_count_tokens",
        (CountTokens, OperationKind::Family(WireFamily::Gemini)) => "gemini_count_tokens",
        (CreateEmbedding, OperationKind::Family(WireFamily::Gemini)) => "gemini_embeddings",
        (CreateEmbedding, _) => "openai_embeddings",
        (Rerank, _) => "openai_rerank",
        (CreateImage, _) => "image_generations",
        (EditImage, _) => "image_edits",
        (CreateSpeech, _) => "openai_audio_speech",
        (CreateTranscription, _) => "openai_audio_transcriptions",
        (CreateTranslation, _) => "openai_audio_translations",
        (CompactContent, _) => "openai_compact",
        (CreateConversation, _) => "openai_conversations",
        (CreateVideo, _) => "openai_video_create",
        (RetrieveVideo, _) => "openai_video_retrieve",
        (ListVideos, _) => "openai_video_list",
        (DeleteVideo, _) => "openai_video_delete",
        (DownloadVideoContent, _) => "openai_video_content",
        (RemixVideo, _) => "openai_video_remix",
        (CreateVideoCharacter, _) => "openai_video_character_create",
        (GetVideoCharacter, _) => "openai_video_character_get",
        (EditVideo, _) => "openai_video_edit",
        (ExtendVideo, _) => "openai_video_extend",
        _ => return None,
    })
}
