use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind, WireFamily};

use crate::TransformError;

#[derive(Clone, Copy)]
pub(crate) enum TransformPair {
    OpenAiModelsToClaude,
    ClaudeModelsToOpenAi,
    OpenAiCountToClaude,
    ClaudeCountToOpenAi,
    ChatToClaude,
    ClaudeToChat,
    ResponsesToClaude,
    ClaudeToResponses,
    OpenAiChatToResponses,
    OpenAiResponsesToChat,
    CompactToClaude,
}

pub(crate) fn resolve(source: OperationKey, target: OperationKey) -> Option<TransformPair> {
    use ContentGenerationKind::{ClaudeMessages, OpenAiChat, OpenAiResponses};
    use OperationKind::{ContentGeneration as Content, Family};
    let pair = match (source.operation, source.kind, target.operation, target.kind) {
        (
            Operation::ListModels | Operation::GetModel,
            Family(WireFamily::OpenAi),
            target_op,
            Family(WireFamily::Claude),
        ) if target_op == source.operation => TransformPair::OpenAiModelsToClaude,
        (
            Operation::ListModels | Operation::GetModel,
            Family(WireFamily::Claude),
            target_op,
            Family(WireFamily::OpenAi),
        ) if target_op == source.operation => TransformPair::ClaudeModelsToOpenAi,
        (
            Operation::CountTokens,
            Family(WireFamily::OpenAi),
            Operation::CountTokens,
            Family(WireFamily::Claude),
        ) => TransformPair::OpenAiCountToClaude,
        (
            Operation::CountTokens,
            Family(WireFamily::Claude),
            Operation::CountTokens,
            Family(WireFamily::OpenAi),
        ) => TransformPair::ClaudeCountToOpenAi,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiChat),
            target_op,
            Content(ClaudeMessages),
        ) if target_op == source.operation => TransformPair::ChatToClaude,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(ClaudeMessages),
            target_op,
            Content(OpenAiChat),
        ) if target_op == source.operation => TransformPair::ClaudeToChat,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiResponses),
            target_op,
            Content(ClaudeMessages),
        ) if target_op == source.operation => TransformPair::ResponsesToClaude,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(ClaudeMessages),
            target_op,
            Content(OpenAiResponses),
        ) if target_op == source.operation => TransformPair::ClaudeToResponses,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiChat),
            target_op,
            Content(OpenAiResponses),
        ) if target_op == source.operation => TransformPair::OpenAiChatToResponses,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiResponses),
            target_op,
            Content(OpenAiChat),
        ) if target_op == source.operation => TransformPair::OpenAiResponsesToChat,
        (
            Operation::GenerateContent,
            Content(ClaudeMessages),
            Operation::StreamGenerateContent,
            Content(OpenAiResponses),
        ) => TransformPair::ClaudeToResponses,
        (
            Operation::GenerateContent,
            Content(OpenAiChat),
            Operation::StreamGenerateContent,
            Content(OpenAiResponses),
        ) => TransformPair::OpenAiChatToResponses,
        (
            Operation::CompactContent,
            Family(WireFamily::OpenAi),
            Operation::GenerateContent,
            Content(ClaudeMessages),
        ) => TransformPair::CompactToClaude,
        _ => return None,
    };
    Some(pair)
}

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
