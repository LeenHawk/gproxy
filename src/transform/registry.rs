use crate::protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind, Provider};

use super::TransformError;

/// Stable identifier for a supported transform implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransformPair {
    OpenAiResponsesToOpenAiChat,
    OpenAiChatToOpenAiResponses,
    OpenAiResponsesToClaudeMessages,
    ClaudeMessagesToOpenAiResponses,
    OpenAiResponsesToGeminiGenerateContent,
    GeminiGenerateContentToOpenAiResponses,
    OpenAiChatToClaudeMessages,
    ClaudeMessagesToOpenAiChat,
    OpenAiChatToGeminiGenerateContent,
    GeminiGenerateContentToOpenAiChat,
    ClaudeMessagesToGeminiGenerateContent,
    GeminiGenerateContentToClaudeMessages,
    OpenAiToClaudeCountTokens,
    ClaudeToOpenAiCountTokens,
    OpenAiToGeminiCountTokens,
    GeminiToOpenAiCountTokens,
    ClaudeToGeminiCountTokens,
    GeminiToClaudeCountTokens,
    OpenAiToClaudeModels,
    ClaudeToOpenAiModels,
    OpenAiToGeminiModels,
    GeminiToOpenAiModels,
    ClaudeToGeminiModels,
    GeminiToClaudeModels,
    OpenAiToGeminiEmbeddings,
    GeminiToOpenAiEmbeddings,
    OpenAiCreateImageToGemini,
    GeminiToOpenAiCreateImage,
    OpenAiCreateImageToOpenAiResponses,
    OpenAiEditImageToGemini,
    GeminiToOpenAiEditImage,
    OpenAiToClaudeCompact,
    ClaudeToOpenAiCompact,
    OpenAiResponsesToOpenAiCompact,
    OpenAiCompactToGemini,
    GeminiToOpenAiCompact,
    OpenAiCompactToOpenAiChat,
    OpenAiChatToOpenAiCompact,
}

/// Resolve operation keys to a concrete pair module.
///
/// Same-kind passthrough is intentionally not represented here; routing should
/// bypass the transform layer when source and target wire kinds match.
pub fn resolve(
    source: OperationKey,
    target: OperationKey,
) -> Result<TransformPair, TransformError> {
    if !source.is_consistent() {
        return Err(TransformError::InvalidInput {
            reason: "source operation and kind are inconsistent".to_owned(),
        });
    }
    if !target.is_consistent() {
        return Err(TransformError::InvalidInput {
            reason: "target operation and kind are inconsistent".to_owned(),
        });
    }
    if source == target {
        return Err(TransformError::unsupported_pair(source, target));
    }
    // Compaction and image generation are the sanctioned cross-operation routes:
    // OpenAI has dedicated compact/image endpoints, while the other providers
    // service them through generate-content.
    if let Some(pair) = resolve_compaction(source, target) {
        return Ok(pair);
    }
    if let Some(pair) = resolve_image_generation(source, target) {
        return Ok(pair);
    }
    if source.operation != target.operation {
        return Err(TransformError::unsupported_pair(source, target));
    }

    match source.operation {
        Operation::GenerateContent | Operation::StreamGenerateContent => {
            resolve_content_generation(source, target)
        }
        Operation::CountTokens => resolve_provider_pair(source, target, count_tokens_pair),
        Operation::ListModels | Operation::GetModel => {
            resolve_provider_pair(source, target, models_pair)
        }
        Operation::CreateEmbedding => resolve_provider_pair(source, target, embeddings_pair),
        // Compaction and image generation are never same-operation routes: OpenAI
        // is the only provider with dedicated compact/image endpoints, and every
        // such conversion is handled cross-operation above.
        Operation::CompactContent
        | Operation::CreateImage
        | Operation::EditImage
        | Operation::CreateConversation => Err(TransformError::unsupported_pair(source, target)),
    }
}

fn resolve_content_generation(
    source: OperationKey,
    target: OperationKey,
) -> Result<TransformPair, TransformError> {
    let OperationKind::ContentGeneration(source_kind) = source.kind else {
        return Err(TransformError::unsupported_pair(source, target));
    };
    let OperationKind::ContentGeneration(target_kind) = target.kind else {
        return Err(TransformError::unsupported_pair(source, target));
    };

    use ContentGenerationKind as Kind;
    use TransformPair as Pair;

    match (source_kind, target_kind) {
        (Kind::OpenAiResponses, Kind::OpenAiChatCompletions) => {
            Ok(Pair::OpenAiResponsesToOpenAiChat)
        }
        (Kind::OpenAiChatCompletions, Kind::OpenAiResponses) => {
            Ok(Pair::OpenAiChatToOpenAiResponses)
        }
        (Kind::OpenAiResponses, Kind::ClaudeMessages) => Ok(Pair::OpenAiResponsesToClaudeMessages),
        (Kind::ClaudeMessages, Kind::OpenAiResponses) => Ok(Pair::ClaudeMessagesToOpenAiResponses),
        (Kind::OpenAiResponses, Kind::GeminiGenerateContent) => {
            Ok(Pair::OpenAiResponsesToGeminiGenerateContent)
        }
        (Kind::GeminiGenerateContent, Kind::OpenAiResponses) => {
            Ok(Pair::GeminiGenerateContentToOpenAiResponses)
        }
        (Kind::OpenAiChatCompletions, Kind::ClaudeMessages) => Ok(Pair::OpenAiChatToClaudeMessages),
        (Kind::ClaudeMessages, Kind::OpenAiChatCompletions) => Ok(Pair::ClaudeMessagesToOpenAiChat),
        (Kind::OpenAiChatCompletions, Kind::GeminiGenerateContent) => {
            Ok(Pair::OpenAiChatToGeminiGenerateContent)
        }
        (Kind::GeminiGenerateContent, Kind::OpenAiChatCompletions) => {
            Ok(Pair::GeminiGenerateContentToOpenAiChat)
        }
        (Kind::ClaudeMessages, Kind::GeminiGenerateContent) => {
            Ok(Pair::ClaudeMessagesToGeminiGenerateContent)
        }
        (Kind::GeminiGenerateContent, Kind::ClaudeMessages) => {
            Ok(Pair::GeminiGenerateContentToClaudeMessages)
        }
        _ => Err(TransformError::unsupported_pair(source, target)),
    }
}

fn resolve_provider_pair(
    source: OperationKey,
    target: OperationKey,
    pair_fn: fn(Provider, Provider) -> Option<TransformPair>,
) -> Result<TransformPair, TransformError> {
    let OperationKind::Provider(source_provider) = source.kind else {
        return Err(TransformError::unsupported_pair(source, target));
    };
    let OperationKind::Provider(target_provider) = target.kind else {
        return Err(TransformError::unsupported_pair(source, target));
    };

    pair_fn(source_provider, target_provider)
        .ok_or_else(|| TransformError::unsupported_pair(source, target))
}

fn count_tokens_pair(source: Provider, target: Provider) -> Option<TransformPair> {
    provider_matrix(
        source,
        target,
        ProviderMatrix {
            openai_to_claude: TransformPair::OpenAiToClaudeCountTokens,
            claude_to_openai: TransformPair::ClaudeToOpenAiCountTokens,
            openai_to_gemini: TransformPair::OpenAiToGeminiCountTokens,
            gemini_to_openai: TransformPair::GeminiToOpenAiCountTokens,
            claude_to_gemini: TransformPair::ClaudeToGeminiCountTokens,
            gemini_to_claude: TransformPair::GeminiToClaudeCountTokens,
        },
    )
}

fn models_pair(source: Provider, target: Provider) -> Option<TransformPair> {
    provider_matrix(
        source,
        target,
        ProviderMatrix {
            openai_to_claude: TransformPair::OpenAiToClaudeModels,
            claude_to_openai: TransformPair::ClaudeToOpenAiModels,
            openai_to_gemini: TransformPair::OpenAiToGeminiModels,
            gemini_to_openai: TransformPair::GeminiToOpenAiModels,
            claude_to_gemini: TransformPair::ClaudeToGeminiModels,
            gemini_to_claude: TransformPair::GeminiToClaudeModels,
        },
    )
}

fn embeddings_pair(source: Provider, target: Provider) -> Option<TransformPair> {
    match (source, target) {
        (Provider::OpenAi, Provider::Gemini) => Some(TransformPair::OpenAiToGeminiEmbeddings),
        (Provider::Gemini, Provider::OpenAi) => Some(TransformPair::GeminiToOpenAiEmbeddings),
        _ => None,
    }
}

/// Resolve cross-operation image-generation routes. OpenAI has dedicated
/// create/edit-image endpoints; Gemini services them through generate-content
/// (image response modality), so it has no image operation of its own.
fn resolve_image_generation(source: OperationKey, target: OperationKey) -> Option<TransformPair> {
    use ContentGenerationKind as Kind;
    use OperationKind as OK;

    // OpenAI create-image -> OpenAI Responses: codex/ChatGPT has no images
    // endpoint and generates via the Responses `image_generation` tool. The same
    // pair serves both directions (request: images->responses; response:
    // responses->images), so resolve it for the reverse ordering too.
    if let (OK::Provider(Provider::OpenAi), OK::ContentGeneration(Kind::OpenAiResponses)) =
        (source.kind, target.kind)
        && target.operation.is_content_generation()
        && source.operation == Operation::CreateImage
    {
        return Some(TransformPair::OpenAiCreateImageToOpenAiResponses);
    }
    if let (OK::ContentGeneration(Kind::OpenAiResponses), OK::Provider(Provider::OpenAi)) =
        (source.kind, target.kind)
        && source.operation.is_content_generation()
        && target.operation == Operation::CreateImage
    {
        return Some(TransformPair::OpenAiCreateImageToOpenAiResponses);
    }

    // OpenAI create/edit image -> Gemini generate-content.
    if let (OK::Provider(Provider::OpenAi), OK::ContentGeneration(Kind::GeminiGenerateContent)) =
        (source.kind, target.kind)
        && target.operation.is_content_generation()
    {
        return match source.operation {
            Operation::CreateImage => Some(TransformPair::OpenAiCreateImageToGemini),
            Operation::EditImage => Some(TransformPair::OpenAiEditImageToGemini),
            _ => None,
        };
    }

    // Gemini generate-content -> OpenAI create/edit image.
    if let (OK::ContentGeneration(Kind::GeminiGenerateContent), OK::Provider(Provider::OpenAi)) =
        (source.kind, target.kind)
        && source.operation.is_content_generation()
    {
        return match target.operation {
            Operation::CreateImage => Some(TransformPair::GeminiToOpenAiCreateImage),
            Operation::EditImage => Some(TransformPair::GeminiToOpenAiEditImage),
            _ => None,
        };
    }

    None
}

/// Resolve cross-operation compaction routes. Compaction is the only sanctioned
/// cross-operation conversion: a generate-content payload folded into an OpenAI
/// compact payload, or an OpenAI compact payload serviced by a generate-content
/// upstream.
fn resolve_compaction(source: OperationKey, target: OperationKey) -> Option<TransformPair> {
    use ContentGenerationKind as Kind;
    use OperationKind as OK;

    // content-generation -> OpenAI compact.
    if let (OK::ContentGeneration(kind), Operation::CompactContent, OK::Provider(Provider::OpenAi)) =
        (source.kind, target.operation, target.kind)
        && source.operation.is_content_generation()
    {
        return match kind {
            Kind::OpenAiResponses => Some(TransformPair::OpenAiResponsesToOpenAiCompact),
            Kind::GeminiGenerateContent => Some(TransformPair::GeminiToOpenAiCompact),
            Kind::OpenAiChatCompletions => Some(TransformPair::OpenAiChatToOpenAiCompact),
            Kind::ClaudeMessages => Some(TransformPair::ClaudeToOpenAiCompact),
        };
    }

    // OpenAI compact -> content-generation.
    if let (OK::Provider(Provider::OpenAi), Operation::CompactContent) =
        (source.kind, source.operation)
        && let OK::ContentGeneration(kind) = target.kind
        && target.operation.is_content_generation()
    {
        return match kind {
            Kind::GeminiGenerateContent => Some(TransformPair::OpenAiCompactToGemini),
            Kind::OpenAiChatCompletions => Some(TransformPair::OpenAiCompactToOpenAiChat),
            Kind::ClaudeMessages => Some(TransformPair::OpenAiToClaudeCompact),
            Kind::OpenAiResponses => None,
        };
    }

    None
}

#[derive(Debug, Clone, Copy)]
struct ProviderMatrix {
    openai_to_claude: TransformPair,
    claude_to_openai: TransformPair,
    openai_to_gemini: TransformPair,
    gemini_to_openai: TransformPair,
    claude_to_gemini: TransformPair,
    gemini_to_claude: TransformPair,
}

fn provider_matrix(
    source: Provider,
    target: Provider,
    matrix: ProviderMatrix,
) -> Option<TransformPair> {
    match (source, target) {
        (Provider::OpenAi, Provider::Claude) => Some(matrix.openai_to_claude),
        (Provider::Claude, Provider::OpenAi) => Some(matrix.claude_to_openai),
        (Provider::OpenAi, Provider::Gemini) => Some(matrix.openai_to_gemini),
        (Provider::Gemini, Provider::OpenAi) => Some(matrix.gemini_to_openai),
        (Provider::Claude, Provider::Gemini) => Some(matrix.claude_to_gemini),
        (Provider::Gemini, Provider::Claude) => Some(matrix.gemini_to_claude),
        _ => None,
    }
}
