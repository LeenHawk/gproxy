//! Typed views over opaque per-provider channel settings.

use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind, Provider};

/// Stable settings key for the exact upstream URL used by a routed operation.
pub fn endpoint_key(op: OperationKey, stream: bool) -> &'static str {
    use ContentGenerationKind as C;
    use Operation as O;
    use Provider as P;

    if let OperationKind::ContentGeneration(kind) = op.kind() {
        return match kind {
            C::OpenAiChatCompletions => "openai_chat_completions",
            C::OpenAiResponses | C::OpenAiResponsesWebSocket => "openai_responses",
            C::ClaudeMessages => "claude_messages",
            C::GeminiGenerateContent if stream => "gemini_stream_generate_content",
            C::GeminiGenerateContent => "gemini_generate_content",
            _ => unreachable!(
                "new non-exhaustive protocol variant requires a lockstep transform update"
            ),
        };
    }

    let OperationKind::Provider(provider) = op.kind() else {
        unreachable!("content generation handled above")
    };
    match (op.operation(), provider) {
        (O::ListModels, P::OpenAi) => "openai_list_models",
        (O::ListModels, P::Claude) => "claude_list_models",
        (O::ListModels, P::Gemini) => "gemini_list_models",
        (O::GetModel, P::OpenAi) => "openai_get_model",
        (O::GetModel, P::Claude) => "claude_get_model",
        (O::GetModel, P::Gemini) => "gemini_get_model",
        (O::CountTokens, P::OpenAi) => "openai_count_tokens",
        (O::CountTokens, P::Claude) => "claude_count_tokens",
        (O::CountTokens, P::Gemini) => "gemini_count_tokens",
        (O::CreateEmbedding, P::OpenAi | P::Claude) => "openai_embeddings",
        (O::CreateEmbedding, P::Gemini) => "gemini_embeddings",
        (O::Rerank, P::OpenAi) => "openai_rerank",
        (O::CreateSpeech, P::OpenAi) => "openai_audio_speech",
        (O::CreateTranscription, P::OpenAi) => "openai_audio_transcriptions",
        (O::CreateTranslation, P::OpenAi) => "openai_audio_translations",
        (O::CreateVideo, P::OpenAi) => "openai_video_create",
        (O::RetrieveVideo, P::OpenAi) => "openai_video_retrieve",
        (O::ListVideos, P::OpenAi) => "openai_video_list",
        (O::DeleteVideo, P::OpenAi) => "openai_video_delete",
        (O::DownloadVideoContent, P::OpenAi) => "openai_video_content",
        (O::RemixVideo, P::OpenAi) => "openai_video_remix",
        (O::CreateVideoCharacter, P::OpenAi) => "openai_video_character_create",
        (O::GetVideoCharacter, P::OpenAi) => "openai_video_character_get",
        (O::EditVideo, P::OpenAi) => "openai_video_edit",
        (O::ExtendVideo, P::OpenAi) => "openai_video_extend",
        (O::CreateImage, _) => "image_generations",
        (O::EditImage, _) => "image_edits",
        (O::WebSearch, _) => "openai_search",
        (O::CompactContent, _) => "openai_compact",
        (O::CreateConversation, _) => "openai_conversations",
        (O::CreateRealtimeCall, _) => "openai_realtime_call",
        (O::ConnectRealtime, _) => "openai_realtime",
        (O::GenerateContent | O::StreamGenerateContent, _) => {
            unreachable!("content operations must carry a content kind")
        }
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

/// Resolve a configured exact endpoint and substitute its optional model slot.
pub fn endpoint_url(
    settings: &Value,
    op: OperationKey,
    stream: bool,
    model: &str,
) -> Option<String> {
    endpoint_by_key(settings, endpoint_key(op, stream), model)
}

/// Resolve an exact endpoint for a concrete request, including resource-id
/// slots used by OpenAI video endpoints.
pub fn endpoint_url_for_request(
    settings: &Value,
    op: OperationKey,
    stream: bool,
    model: &str,
    path: &str,
) -> Option<String> {
    let url = endpoint_url(settings, op, stream, model)?;
    let resource = match op.operation() {
        Operation::RetrieveVideo
        | Operation::DeleteVideo
        | Operation::DownloadVideoContent
        | Operation::RemixVideo => video_id(path).map(|id| ("{video_id}", id)),
        Operation::GetVideoCharacter => path
            .strip_prefix("/v1/videos/characters/")
            .filter(|id| !id.is_empty() && !id.contains('/'))
            .map(|id| ("{character_id}", id)),
        _ => None,
    };
    match resource {
        Some((slot, id)) => Some(url.replace(slot, &crate::channel::oauth::percent_encode(id))),
        None => Some(url),
    }
}

fn video_id(path: &str) -> Option<&str> {
    path.strip_prefix("/v1/videos/")?
        .split('/')
        .next()
        .filter(|id| !id.is_empty() && *id != "characters")
}

pub fn endpoint_by_key(settings: &Value, key: &str, model: &str) -> Option<String> {
    settings
        .get("endpoints")
        .and_then(Value::as_object)
        .and_then(|endpoints| endpoints.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(|url| url.replace("{model}", model))
}

/// Settings shared by request-shaping implementations across bulletins.
///
/// Unknown fields remain private to their bulletin. Missing fields retain the
/// historical opt-in behavior through serde defaults.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RequestShapeSettings {
    #[serde(deserialize_with = "bool_or_default")]
    pub enable_openai_magic_cache: bool,
    #[serde(deserialize_with = "bool_or_default")]
    pub enable_claude_magic_cache: bool,
    pub claude_fable_fallbacks: Option<ClaudeFableFallbacks>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ClaudeFableFallbacks {
    Default(ClaudeFallbackDefault),
    Models(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
pub enum ClaudeFallbackDefault {
    #[serde(rename = "default")]
    Default,
}

impl RequestShapeSettings {
    pub fn from_value(value: &Value) -> Self {
        Self::deserialize(value).unwrap_or_default()
    }
}

fn bool_or_default<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Value::deserialize(deserializer)?.as_bool().unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn video_endpoint_substitutes_resource_identifier() {
        let settings = json!({
            "endpoints": {
                "openai_video_content": "https://media.example/videos/{video_id}/content",
                "openai_video_character_get": "https://media.example/characters/{character_id}"
            }
        });
        let video = OperationKey::provider(Operation::DownloadVideoContent, Provider::OpenAi);
        assert_eq!(
            endpoint_url_for_request(&settings, video, false, "", "/v1/videos/video 123/content")
                .as_deref(),
            Some("https://media.example/videos/video%20123/content")
        );
        let character = OperationKey::provider(Operation::GetVideoCharacter, Provider::OpenAi);
        assert_eq!(
            endpoint_url_for_request(
                &settings,
                character,
                false,
                "",
                "/v1/videos/characters/char_123"
            )
            .as_deref(),
            Some("https://media.example/characters/char_123")
        );
    }
}
