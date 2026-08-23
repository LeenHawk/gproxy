use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind, WireFamily};

use super::TransformPair;

pub(crate) fn resolve(source: OperationKey, target: OperationKey) -> Option<TransformPair> {
    use ContentGenerationKind::{
        ClaudeMessages, GeminiGenerateContent, OpenAiChat, OpenAiResponses,
    };
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
            Content(ClaudeMessages),
            target_op,
            Content(GeminiGenerateContent),
        ) if target_op == source.operation => TransformPair::ClaudeToGemini,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(GeminiGenerateContent),
            target_op,
            Content(ClaudeMessages),
        ) if target_op == source.operation => TransformPair::GeminiToClaude,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(GeminiGenerateContent),
            target_op,
            Content(OpenAiChat),
        ) if target_op == source.operation => TransformPair::GeminiToChat,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiChat),
            target_op,
            Content(GeminiGenerateContent),
        ) if target_op == source.operation => TransformPair::ChatToGemini,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(GeminiGenerateContent),
            target_op,
            Content(OpenAiResponses),
        ) if target_op == source.operation => TransformPair::GeminiToResponses,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiResponses),
            target_op,
            Content(GeminiGenerateContent),
        ) if target_op == source.operation => TransformPair::ResponsesToGemini,
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
            Operation::GenerateContent,
            Content(GeminiGenerateContent),
            Operation::StreamGenerateContent,
            Content(OpenAiResponses),
        ) => TransformPair::GeminiToResponses,
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
