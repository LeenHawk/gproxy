use bytes::Bytes;

use super::TransformPair;
use crate::TransformError;

pub(crate) fn request(
    pair: TransformPair,
    body: Bytes,
    model: &str,
    stream: bool,
) -> Result<Bytes, TransformError> {
    match pair {
        TransformPair::OpenAiModelsToClaude => {
            crate::models::openai_to_claude::request::transform(body)
        }
        TransformPair::ClaudeModelsToOpenAi => {
            crate::models::claude_to_openai::request::transform(body)
        }
        TransformPair::OpenAiCountToClaude => {
            crate::count_tokens::openai_to_claude::request::transform(body, model)
        }
        TransformPair::ClaudeCountToOpenAi => {
            crate::count_tokens::claude_to_openai::request::transform(body, model)
        }
        TransformPair::ChatToClaude => {
            crate::generate_content::openai_chat_to_claude_messages::request::transform(
                body, model, stream,
            )
        }
        TransformPair::ClaudeToChat => {
            crate::generate_content::claude_messages_to_openai_chat::request::transform(
                body, model, stream,
            )
        }
        TransformPair::ResponsesToClaude => {
            crate::generate_content::openai_responses_to_claude_messages::request::transform(
                body, model, stream,
            )
        }
        TransformPair::ClaudeToResponses => {
            crate::generate_content::claude_messages_to_openai_responses::request::transform(
                body, model, stream,
            )
        }
        TransformPair::ClaudeToGemini => {
            crate::generate_content::claude_messages_to_gemini_generate_content::request::transform(
                body, model, stream,
            )
        }
        TransformPair::GeminiToClaude => {
            crate::generate_content::gemini_generate_content_to_claude_messages::request::transform(
                body, model, stream,
            )
        }
        TransformPair::GeminiToChat => {
            crate::generate_content::gemini_generate_content_to_openai_chat::request::transform(
                body, model, stream,
            )
        }
        TransformPair::ChatToGemini => {
            crate::generate_content::openai_chat_to_gemini_generate_content::request::transform(
                body, model, stream,
            )
        }
        TransformPair::GeminiToResponses => {
            crate::generate_content::gemini_generate_content_to_openai_responses::request::transform(
                body, model, stream,
            )
        }
        TransformPair::ResponsesToGemini => {
            crate::generate_content::openai_responses_to_gemini_generate_content::request::transform(
                body, model, stream,
            )
        }
        TransformPair::CompactToClaude => {
            crate::compact::openai_to_claude_messages::request::transform(body, model)
        }
        TransformPair::OpenAiChatToResponses => {
            crate::generate_content::openai_chat_to_openai_responses::request::transform(
                body, model, stream,
            )
        }
        TransformPair::OpenAiResponsesToChat => {
            crate::generate_content::openai_responses_to_openai_chat::request::transform(
                body, model, stream,
            )
        }
    }
}

pub(crate) fn response(pair: TransformPair, body: Bytes) -> Result<Bytes, TransformError> {
    match pair {
        TransformPair::OpenAiModelsToClaude => {
            crate::models::openai_to_claude::response::transform(body)
        }
        TransformPair::ClaudeModelsToOpenAi => {
            crate::models::claude_to_openai::response::transform(body)
        }
        TransformPair::OpenAiCountToClaude => {
            crate::count_tokens::openai_to_claude::response::transform(body)
        }
        TransformPair::ClaudeCountToOpenAi => {
            crate::count_tokens::claude_to_openai::response::transform(body)
        }
        TransformPair::ChatToClaude => {
            crate::generate_content::openai_chat_to_claude_messages::response::transform(body)
        }
        TransformPair::ClaudeToChat => {
            crate::generate_content::claude_messages_to_openai_chat::response::transform(body)
        }
        TransformPair::ResponsesToClaude => {
            crate::generate_content::openai_responses_to_claude_messages::response::transform(body)
        }
        TransformPair::ClaudeToResponses => {
            crate::generate_content::claude_messages_to_openai_responses::response::transform(body)
        }
        TransformPair::ClaudeToGemini => {
            crate::generate_content::gemini_generate_content_to_claude_messages::response::transform(
                body,
            )
        }
        TransformPair::GeminiToClaude => {
            crate::generate_content::claude_messages_to_gemini_generate_content::response::transform(
                body,
            )
        }
        TransformPair::GeminiToChat => {
            crate::generate_content::gemini_generate_content_to_openai_chat::response::transform(
                body,
            )
        }
        TransformPair::ChatToGemini => {
            crate::generate_content::openai_chat_to_gemini_generate_content::response::transform(
                body,
            )
        }
        TransformPair::GeminiToResponses => {
            crate::generate_content::gemini_generate_content_to_openai_responses::response::transform(
                body,
            )
        }
        TransformPair::ResponsesToGemini => {
            crate::generate_content::openai_responses_to_gemini_generate_content::response::transform(
                body,
            )
        }
        TransformPair::CompactToClaude => {
            crate::compact::openai_to_claude_messages::response::transform(body)
        }
        TransformPair::OpenAiChatToResponses => {
            crate::generate_content::openai_chat_to_openai_responses::response::transform(body)
        }
        TransformPair::OpenAiResponsesToChat => {
            crate::generate_content::openai_responses_to_openai_chat::response::transform(body)
        }
    }
}
