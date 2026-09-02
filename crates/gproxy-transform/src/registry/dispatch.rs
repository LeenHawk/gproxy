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
        TransformPair::OpenAiModelsToGemini
        | TransformPair::GeminiModelsToOpenAi
        | TransformPair::ClaudeModelsToGemini
        | TransformPair::GeminiModelsToClaude => Ok(Bytes::new()),
        TransformPair::OpenAiCountToClaude => {
            crate::count_tokens::openai_to_claude::request::transform(body, model)
        }
        TransformPair::ClaudeCountToOpenAi => {
            crate::count_tokens::claude_to_openai::request::transform(body, model)
        }
        TransformPair::OpenAiCountToGemini => crate::count_tokens::gemini::request(
            gproxy_protocol::WireFamily::OpenAi,
            gproxy_protocol::WireFamily::Gemini,
            body,
            model,
        ),
        TransformPair::GeminiCountToOpenAi => crate::count_tokens::gemini::request(
            gproxy_protocol::WireFamily::Gemini,
            gproxy_protocol::WireFamily::OpenAi,
            body,
            model,
        ),
        TransformPair::ClaudeCountToGemini => crate::count_tokens::gemini::request(
            gproxy_protocol::WireFamily::Claude,
            gproxy_protocol::WireFamily::Gemini,
            body,
            model,
        ),
        TransformPair::GeminiCountToClaude => crate::count_tokens::gemini::request(
            gproxy_protocol::WireFamily::Gemini,
            gproxy_protocol::WireFamily::Claude,
            body,
            model,
        ),
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
        TransformPair::CompactToResponses => crate::compact::other::compact_request(
            gproxy_protocol::ContentGenerationKind::OpenAiResponses,
            body,
            model,
            stream,
        ),
        TransformPair::CompactToGemini => crate::compact::other::compact_request(
            gproxy_protocol::ContentGenerationKind::GeminiGenerateContent,
            body,
            model,
            stream,
        ),
        TransformPair::CompactToChat => crate::compact::other::compact_request(
            gproxy_protocol::ContentGenerationKind::OpenAiChat,
            body,
            model,
            stream,
        ),
        TransformPair::ResponsesToCompact => crate::compact::other::content_request(
            gproxy_protocol::ContentGenerationKind::OpenAiResponses,
            body,
            model,
        ),
        TransformPair::GeminiToCompact => crate::compact::other::content_request(
            gproxy_protocol::ContentGenerationKind::GeminiGenerateContent,
            body,
            model,
        ),
        TransformPair::ChatToCompact => crate::compact::other::content_request(
            gproxy_protocol::ContentGenerationKind::OpenAiChat,
            body,
            model,
        ),
        TransformPair::ClaudeToCompact => crate::compact::other::content_request(
            gproxy_protocol::ContentGenerationKind::ClaudeMessages,
            body,
            model,
        ),
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
        TransformPair::OpenAiEmbeddingToGemini => {
            crate::embeddings::openai_to_gemini_single(body, model)
        }
        TransformPair::OpenAiEmbeddingToGeminiBatch => {
            crate::embeddings::openai_to_gemini_batch(body, model)
        }
        TransformPair::GeminiEmbeddingToOpenAi => {
            crate::embeddings::gemini_single_to_openai(body, model)
        }
        TransformPair::GeminiBatchEmbeddingToOpenAi => {
            crate::embeddings::gemini_batch_to_openai(body, model)
        }
        TransformPair::OpenAiCreateImageToGemini => {
            crate::images::generate_content::openai_request(body, model, false)
        }
        TransformPair::OpenAiEditImageToGemini => {
            crate::images::generate_content::openai_request(body, model, true)
        }
        TransformPair::GeminiToOpenAiCreateImage => {
            crate::images::generate_content::gemini_request(body, model, false)
        }
        TransformPair::GeminiToOpenAiEditImage => {
            crate::images::generate_content::gemini_request(body, model, true)
        }
        TransformPair::OpenAiCreateImageToResponses => {
            crate::images::responses::image_request(body, model, false)
        }
        TransformPair::OpenAiEditImageToResponses => {
            crate::images::responses::image_request(body, model, true)
        }
        TransformPair::ResponsesToOpenAiCreateImage => {
            crate::images::responses::responses_request(body, model, false)
        }
        TransformPair::ResponsesToOpenAiEditImage => {
            crate::images::responses::responses_request(body, model, true)
        }
        TransformPair::OpenAiCreateImageToImagen => crate::images::imagen::openai_request(body),
        TransformPair::ImagenToOpenAiCreateImage => {
            crate::images::imagen::gemini_request(body, model)
        }
        TransformPair::OpenAiVideoToGemini => crate::videos::openai_request(body),
        TransformPair::GeminiVideoToOpenAi => crate::videos::gemini_request(body, model),
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
        TransformPair::OpenAiModelsToGemini => crate::models::gemini::response(
            gproxy_protocol::WireFamily::OpenAi,
            gproxy_protocol::WireFamily::Gemini,
            body,
        ),
        TransformPair::GeminiModelsToOpenAi => crate::models::gemini::response(
            gproxy_protocol::WireFamily::Gemini,
            gproxy_protocol::WireFamily::OpenAi,
            body,
        ),
        TransformPair::ClaudeModelsToGemini => crate::models::gemini::response(
            gproxy_protocol::WireFamily::Claude,
            gproxy_protocol::WireFamily::Gemini,
            body,
        ),
        TransformPair::GeminiModelsToClaude => crate::models::gemini::response(
            gproxy_protocol::WireFamily::Gemini,
            gproxy_protocol::WireFamily::Claude,
            body,
        ),
        TransformPair::OpenAiCountToClaude => {
            crate::count_tokens::openai_to_claude::response::transform(body)
        }
        TransformPair::ClaudeCountToOpenAi => {
            crate::count_tokens::claude_to_openai::response::transform(body)
        }
        TransformPair::OpenAiCountToGemini => crate::count_tokens::gemini::response(
            gproxy_protocol::WireFamily::OpenAi,
            gproxy_protocol::WireFamily::Gemini,
            body,
        ),
        TransformPair::GeminiCountToOpenAi => crate::count_tokens::gemini::response(
            gproxy_protocol::WireFamily::Gemini,
            gproxy_protocol::WireFamily::OpenAi,
            body,
        ),
        TransformPair::ClaudeCountToGemini => crate::count_tokens::gemini::response(
            gproxy_protocol::WireFamily::Claude,
            gproxy_protocol::WireFamily::Gemini,
            body,
        ),
        TransformPair::GeminiCountToClaude => crate::count_tokens::gemini::response(
            gproxy_protocol::WireFamily::Gemini,
            gproxy_protocol::WireFamily::Claude,
            body,
        ),
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
        TransformPair::CompactToResponses => crate::compact::other::compact_response(
            gproxy_protocol::ContentGenerationKind::OpenAiResponses,
            body,
        ),
        TransformPair::CompactToGemini => crate::compact::other::compact_response(
            gproxy_protocol::ContentGenerationKind::GeminiGenerateContent,
            body,
        ),
        TransformPair::CompactToChat => crate::compact::other::compact_response(
            gproxy_protocol::ContentGenerationKind::OpenAiChat,
            body,
        ),
        TransformPair::ResponsesToCompact => crate::compact::other::content_response(
            gproxy_protocol::ContentGenerationKind::OpenAiResponses,
            body,
        ),
        TransformPair::GeminiToCompact => crate::compact::other::content_response(
            gproxy_protocol::ContentGenerationKind::GeminiGenerateContent,
            body,
        ),
        TransformPair::ChatToCompact => crate::compact::other::content_response(
            gproxy_protocol::ContentGenerationKind::OpenAiChat,
            body,
        ),
        TransformPair::ClaudeToCompact => crate::compact::other::content_response(
            gproxy_protocol::ContentGenerationKind::ClaudeMessages,
            body,
        ),
        TransformPair::OpenAiChatToResponses => {
            crate::generate_content::openai_chat_to_openai_responses::response::transform(body)
        }
        TransformPair::OpenAiResponsesToChat => {
            crate::generate_content::openai_responses_to_openai_chat::response::transform(body)
        }
        TransformPair::OpenAiEmbeddingToGemini => {
            crate::embeddings::gemini_single_response_to_openai(body)
        }
        TransformPair::OpenAiEmbeddingToGeminiBatch => {
            crate::embeddings::gemini_batch_response_to_openai(body)
        }
        TransformPair::GeminiEmbeddingToOpenAi => {
            crate::embeddings::openai_response_to_gemini_single(body)
        }
        TransformPair::GeminiBatchEmbeddingToOpenAi => {
            crate::embeddings::openai_response_to_gemini_batch(body)
        }
        TransformPair::OpenAiCreateImageToGemini | TransformPair::OpenAiEditImageToGemini => {
            crate::images::generate_content::gemini_response_to_openai(body)
        }
        TransformPair::GeminiToOpenAiCreateImage | TransformPair::GeminiToOpenAiEditImage => {
            crate::images::generate_content::openai_response_to_gemini(body)
        }
        TransformPair::OpenAiCreateImageToResponses | TransformPair::OpenAiEditImageToResponses => {
            crate::images::responses::responses_to_images(body)
        }
        TransformPair::ResponsesToOpenAiCreateImage | TransformPair::ResponsesToOpenAiEditImage => {
            crate::images::responses::images_to_responses(body)
        }
        TransformPair::OpenAiCreateImageToImagen => {
            crate::images::imagen::gemini_response_to_openai(body)
        }
        TransformPair::ImagenToOpenAiCreateImage => {
            crate::images::imagen::openai_response_to_gemini(body)
        }
        TransformPair::OpenAiVideoToGemini => crate::videos::gemini_response_to_openai(body),
        TransformPair::GeminiVideoToOpenAi => crate::videos::openai_response_to_gemini(body),
    }
}
