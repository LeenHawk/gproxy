use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind, WireFamily};

use super::TransformPair;

pub(crate) fn resolve(source: OperationKey, target: OperationKey) -> Option<TransformPair> {
    let (source, target) = normalize_content_operations(source, target);
    use ContentGenerationKind::{
        ClaudeMessages, GeminiGenerateContent, OpenAiChat, OpenAiResponses,
    };
    use OperationKind::{ContentGeneration as Content, Family};
    let pair = match (
        source.operation(),
        source.kind(),
        target.operation(),
        target.kind(),
    ) {
        (
            Operation::ListModels | Operation::GetModel,
            Family(WireFamily::OpenAi),
            target_op,
            Family(WireFamily::Claude),
        ) if target_op == source.operation() => TransformPair::OpenAiModelsToClaude,
        (
            Operation::ListModels | Operation::GetModel,
            Family(WireFamily::Claude),
            target_op,
            Family(WireFamily::OpenAi),
        ) if target_op == source.operation() => TransformPair::ClaudeModelsToOpenAi,
        (
            Operation::ListModels | Operation::GetModel,
            Family(WireFamily::OpenAi),
            target_op,
            Family(WireFamily::Gemini),
        ) if target_op == source.operation() => TransformPair::OpenAiModelsToGemini,
        (
            Operation::ListModels | Operation::GetModel,
            Family(WireFamily::Gemini),
            target_op,
            Family(WireFamily::OpenAi),
        ) if target_op == source.operation() => TransformPair::GeminiModelsToOpenAi,
        (
            Operation::ListModels | Operation::GetModel,
            Family(WireFamily::Claude),
            target_op,
            Family(WireFamily::Gemini),
        ) if target_op == source.operation() => TransformPair::ClaudeModelsToGemini,
        (
            Operation::ListModels | Operation::GetModel,
            Family(WireFamily::Gemini),
            target_op,
            Family(WireFamily::Claude),
        ) if target_op == source.operation() => TransformPair::GeminiModelsToClaude,
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
            Operation::CountTokens,
            Family(WireFamily::OpenAi),
            Operation::CountTokens,
            Family(WireFamily::Gemini),
        ) => TransformPair::OpenAiCountToGemini,
        (
            Operation::CountTokens,
            Family(WireFamily::Gemini),
            Operation::CountTokens,
            Family(WireFamily::OpenAi),
        ) => TransformPair::GeminiCountToOpenAi,
        (
            Operation::CountTokens,
            Family(WireFamily::Claude),
            Operation::CountTokens,
            Family(WireFamily::Gemini),
        ) => TransformPair::ClaudeCountToGemini,
        (
            Operation::CountTokens,
            Family(WireFamily::Gemini),
            Operation::CountTokens,
            Family(WireFamily::Claude),
        ) => TransformPair::GeminiCountToClaude,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiChat),
            target_op,
            Content(ClaudeMessages),
        ) if target_op == source.operation() => TransformPair::ChatToClaude,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(ClaudeMessages),
            target_op,
            Content(OpenAiChat),
        ) if target_op == source.operation() => TransformPair::ClaudeToChat,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiResponses),
            target_op,
            Content(ClaudeMessages),
        ) if target_op == source.operation() => TransformPair::ResponsesToClaude,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(ClaudeMessages),
            target_op,
            Content(OpenAiResponses),
        ) if target_op == source.operation() => TransformPair::ClaudeToResponses,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(ClaudeMessages),
            target_op,
            Content(GeminiGenerateContent),
        ) if target_op == source.operation() => TransformPair::ClaudeToGemini,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(GeminiGenerateContent),
            target_op,
            Content(ClaudeMessages),
        ) if target_op == source.operation() => TransformPair::GeminiToClaude,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(GeminiGenerateContent),
            target_op,
            Content(OpenAiChat),
        ) if target_op == source.operation() => TransformPair::GeminiToChat,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiChat),
            target_op,
            Content(GeminiGenerateContent),
        ) if target_op == source.operation() => TransformPair::ChatToGemini,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(GeminiGenerateContent),
            target_op,
            Content(OpenAiResponses),
        ) if target_op == source.operation() => TransformPair::GeminiToResponses,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiResponses),
            target_op,
            Content(GeminiGenerateContent),
        ) if target_op == source.operation() => TransformPair::ResponsesToGemini,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiChat),
            target_op,
            Content(OpenAiResponses),
        ) if target_op == source.operation() => TransformPair::OpenAiChatToResponses,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiResponses),
            target_op,
            Content(OpenAiChat),
        ) if target_op == source.operation() => TransformPair::OpenAiResponsesToChat,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
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
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(GeminiGenerateContent),
            Operation::StreamGenerateContent,
            Content(OpenAiResponses),
        ) => TransformPair::GeminiToResponses,
        (
            Operation::CompactContent,
            Family(WireFamily::OpenAi),
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(ClaudeMessages),
        ) => TransformPair::CompactToClaude,
        (
            Operation::CompactContent,
            Family(WireFamily::OpenAi),
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiResponses),
        ) => TransformPair::CompactToResponses,
        (
            Operation::CompactContent,
            Family(WireFamily::OpenAi),
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(GeminiGenerateContent),
        ) => TransformPair::CompactToGemini,
        (
            Operation::CompactContent,
            Family(WireFamily::OpenAi),
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiChat),
        ) => TransformPair::CompactToChat,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiResponses),
            Operation::CompactContent,
            Family(WireFamily::OpenAi),
        ) => TransformPair::ResponsesToCompact,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(GeminiGenerateContent),
            Operation::CompactContent,
            Family(WireFamily::OpenAi),
        ) => TransformPair::GeminiToCompact,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiChat),
            Operation::CompactContent,
            Family(WireFamily::OpenAi),
        ) => TransformPair::ChatToCompact,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(ClaudeMessages),
            Operation::CompactContent,
            Family(WireFamily::OpenAi),
        ) => TransformPair::ClaudeToCompact,
        (
            Operation::CreateEmbedding,
            Family(WireFamily::OpenAi),
            Operation::CreateEmbedding,
            Family(WireFamily::Gemini),
        ) => TransformPair::OpenAiEmbeddingToGemini,
        (
            Operation::CreateEmbedding,
            Family(WireFamily::OpenAi),
            Operation::BatchCreateEmbedding,
            Family(WireFamily::Gemini),
        ) => TransformPair::OpenAiEmbeddingToGeminiBatch,
        (
            Operation::CreateEmbedding,
            Family(WireFamily::Gemini),
            Operation::CreateEmbedding,
            Family(WireFamily::OpenAi),
        ) => TransformPair::GeminiEmbeddingToOpenAi,
        (
            Operation::BatchCreateEmbedding,
            Family(WireFamily::Gemini),
            Operation::CreateEmbedding,
            Family(WireFamily::OpenAi),
        ) => TransformPair::GeminiBatchEmbeddingToOpenAi,
        (
            Operation::CreateImage,
            Family(WireFamily::OpenAi),
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(GeminiGenerateContent),
        ) => TransformPair::OpenAiCreateImageToGemini,
        (
            Operation::GenerateContent,
            Content(GeminiGenerateContent),
            Operation::CreateImage,
            Family(WireFamily::OpenAi),
        ) => TransformPair::GeminiToOpenAiCreateImage,
        (
            Operation::EditImage,
            Family(WireFamily::OpenAi),
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(GeminiGenerateContent),
        ) => TransformPair::OpenAiEditImageToGemini,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(GeminiGenerateContent),
            Operation::EditImage,
            Family(WireFamily::OpenAi),
        ) => TransformPair::GeminiToOpenAiEditImage,
        (
            Operation::CreateImage,
            Family(WireFamily::OpenAi),
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiResponses),
        ) => TransformPair::OpenAiCreateImageToResponses,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiResponses),
            Operation::CreateImage,
            Family(WireFamily::OpenAi),
        ) => TransformPair::ResponsesToOpenAiCreateImage,
        (
            Operation::EditImage,
            Family(WireFamily::OpenAi),
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiResponses),
        ) => TransformPair::OpenAiEditImageToResponses,
        (
            Operation::GenerateContent | Operation::StreamGenerateContent,
            Content(OpenAiResponses),
            Operation::EditImage,
            Family(WireFamily::OpenAi),
        ) => TransformPair::ResponsesToOpenAiEditImage,
        (
            Operation::CreateImage,
            Family(WireFamily::OpenAi),
            Operation::CreateImage,
            Family(WireFamily::Gemini),
        ) => TransformPair::OpenAiCreateImageToImagen,
        (
            Operation::CreateImage,
            Family(WireFamily::Gemini),
            Operation::CreateImage,
            Family(WireFamily::OpenAi),
        ) => TransformPair::ImagenToOpenAiCreateImage,
        (
            Operation::CreateVideo | Operation::RetrieveVideo,
            Family(WireFamily::OpenAi),
            target_operation,
            Family(WireFamily::Gemini),
        ) if target_operation == source.operation() => TransformPair::OpenAiVideoToGemini,
        (
            Operation::CreateVideo | Operation::RetrieveVideo,
            Family(WireFamily::Gemini),
            target_operation,
            Family(WireFamily::OpenAi),
        ) if target_operation == source.operation() => TransformPair::GeminiVideoToOpenAi,
        _ => return None,
    };
    Some(pair)
}

fn normalize_content_operations(
    source: OperationKey,
    target: OperationKey,
) -> (OperationKey, OperationKey) {
    use ContentGenerationKind::{OpenAiResponses, OpenAiResponsesWebSocket};
    use OperationKind::ContentGeneration;
    let source_kind = if let ContentGeneration(kind) = source.kind() {
        ContentGeneration(if kind == OpenAiResponsesWebSocket {
            OpenAiResponses
        } else {
            kind
        })
    } else {
        source.kind()
    };
    let target_kind = if let ContentGeneration(kind) = target.kind() {
        ContentGeneration(if kind == OpenAiResponsesWebSocket {
            OpenAiResponses
        } else {
            kind
        })
    } else {
        target.kind()
    };
    let both_content = matches!(
        source.operation(),
        Operation::GenerateContent | Operation::StreamGenerateContent
    ) && matches!(
        target.operation(),
        Operation::GenerateContent | Operation::StreamGenerateContent
    );
    let source_operation = if both_content {
        Operation::GenerateContent
    } else {
        source.operation()
    };
    let target_operation = if both_content {
        Operation::GenerateContent
    } else {
        target.operation()
    };
    (
        OperationKey::try_new(source_operation, source_kind).expect("normalized source key"),
        OperationKey::try_new(target_operation, target_kind).expect("normalized target key"),
    )
}
