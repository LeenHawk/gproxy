//! Typed views over opaque per-provider channel settings.

use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind, Provider};

/// Stable settings key for the exact upstream URL used by a routed operation.
pub fn endpoint_key(op: OperationKey, stream: bool) -> &'static str {
    use ContentGenerationKind as C;
    use Operation as O;
    use Provider as P;

    if let OperationKind::ContentGeneration(kind) = op.kind {
        return match kind {
            C::OpenAiChatCompletions => "openai_chat_completions",
            C::OpenAiResponses | C::OpenAiResponsesWebSocket => "openai_responses",
            C::ClaudeMessages => "claude_messages",
            C::GeminiGenerateContent if stream => "gemini_stream_generate_content",
            C::GeminiGenerateContent => "gemini_generate_content",
        };
    }

    let OperationKind::Provider(provider) = op.kind else {
        unreachable!("content generation handled above")
    };
    match (op.operation, provider) {
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
        (O::CreateImage, _) => "image_generations",
        (O::EditImage, _) => "image_edits",
        (O::CompactContent, _) => "openai_compact",
        (O::CreateConversation, _) => "openai_conversations",
        (O::GenerateContent | O::StreamGenerateContent, _) => {
            unreachable!("content operations must carry a content kind")
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
    pub enable_magic_cache: bool,
    #[serde(deserialize_with = "bool_or_default")]
    pub enable_claude_fable_fallback: bool,
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
