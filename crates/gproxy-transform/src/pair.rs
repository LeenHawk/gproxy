use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind, WireFamily};

use crate::TransformError;

#[derive(Clone, Copy)]
pub(crate) enum Pair {
    OpenAiModelsToClaude,
    ClaudeModelsToOpenAi,
    OpenAiCountToClaude,
    ClaudeCountToOpenAi,
    ChatToClaude,
    ClaudeToChat,
    ResponsesToClaude,
    ClaudeToResponses,
    CompactToClaude,
}

pub(crate) fn resolve(source: OperationKey, target: OperationKey) -> Option<Pair> {
    use ContentGenerationKind::{ClaudeMessages, OpenAiChat, OpenAiResponses};
    use OperationKind::{ContentGeneration as Content, Family};
    let pair = match (source.operation, source.kind, target.operation, target.kind) {
        (
            Operation::ListModels | Operation::GetModel,
            Family(WireFamily::OpenAi),
            target_op,
            Family(WireFamily::Claude),
        ) if target_op == source.operation => Pair::OpenAiModelsToClaude,
        (
            Operation::ListModels | Operation::GetModel,
            Family(WireFamily::Claude),
            target_op,
            Family(WireFamily::OpenAi),
        ) if target_op == source.operation => Pair::ClaudeModelsToOpenAi,
        (
            Operation::CountTokens,
            Family(WireFamily::OpenAi),
            Operation::CountTokens,
            Family(WireFamily::Claude),
        ) => Pair::OpenAiCountToClaude,
        (
            Operation::CountTokens,
            Family(WireFamily::Claude),
            Operation::CountTokens,
            Family(WireFamily::OpenAi),
        ) => Pair::ClaudeCountToOpenAi,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiChat),
            target_op,
            Content(ClaudeMessages),
        ) if target_op == source.operation => Pair::ChatToClaude,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(ClaudeMessages),
            target_op,
            Content(OpenAiChat),
        ) if target_op == source.operation => Pair::ClaudeToChat,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiResponses),
            target_op,
            Content(ClaudeMessages),
        ) if target_op == source.operation => Pair::ResponsesToClaude,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(ClaudeMessages),
            target_op,
            Content(OpenAiResponses),
        ) if target_op == source.operation => Pair::ClaudeToResponses,
        (
            Operation::CompactContent,
            Family(WireFamily::OpenAi),
            Operation::GenerateContent,
            Content(ClaudeMessages),
        ) => Pair::CompactToClaude,
        _ => return None,
    };
    Some(pair)
}

pub(crate) fn request(
    pair: Pair,
    body: Bytes,
    model: &str,
    stream: bool,
) -> Result<Bytes, TransformError> {
    match pair {
        Pair::OpenAiModelsToClaude | Pair::ClaudeModelsToOpenAi => Ok(body),
        Pair::OpenAiCountToClaude => crate::count_tokens::openai_to_claude(body, model),
        Pair::ClaudeCountToOpenAi => crate::count_tokens::claude_to_openai(body, model),
        Pair::ChatToClaude => crate::content::chat_to_claude(body, model, stream),
        Pair::ClaudeToChat => crate::content::claude_to_chat(body, model, stream),
        Pair::ResponsesToClaude => crate::content::responses_to_claude(body, model, stream),
        Pair::ClaudeToResponses => crate::content::claude_to_responses(body, model, stream),
        Pair::CompactToClaude => crate::compact::request(body, model),
    }
}

pub(crate) fn response(pair: Pair, body: Bytes) -> Result<Bytes, TransformError> {
    match pair {
        Pair::OpenAiModelsToClaude => crate::models::claude_to_openai_response(body),
        Pair::ClaudeModelsToOpenAi => crate::models::openai_to_claude_response(body),
        Pair::OpenAiCountToClaude => crate::count_tokens::claude_to_openai_response(body),
        Pair::ClaudeCountToOpenAi => crate::count_tokens::openai_to_claude_response(body),
        Pair::ChatToClaude => crate::content::claude_to_chat_response(body),
        Pair::ClaudeToChat => crate::content::chat_to_claude_response(body),
        Pair::ResponsesToClaude => crate::content::claude_to_responses_response(body),
        Pair::ClaudeToResponses => crate::content::responses_to_claude_response(body),
        Pair::CompactToClaude => crate::compact::response(body),
    }
}
