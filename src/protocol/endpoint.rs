//! Target endpoint synthesis (M2): the provider-relative method/path/query a
//! transformed request must hit for a given operation key. Passthrough keeps
//! the inbound target and never calls this.

use crate::protocol::operation::{
    ContentGenerationKind, HttpMethod, Operation, OperationKey, OperationKind, Provider,
};

/// Provider-relative request target for a wired operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestTarget {
    pub method: HttpMethod,
    pub path: String,
    /// Extra query the wire format requires (e.g. gemini `alt=sse`).
    pub query: Option<String>,
}

impl RequestTarget {
    fn get(path: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Get,
            path: path.into(),
            query: None,
        }
    }

    fn post(path: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Post,
            path: path.into(),
            query: None,
        }
    }
}

/// Build the upstream request target for any wired operation key. `model` is
/// the upstream model id (path-templated providers embed it); `stream` selects
/// the streaming variant where the wire format distinguishes it by endpoint.
pub fn request_target(target: OperationKey, model: &str, stream: bool) -> RequestTarget {
    use Provider as P;
    let provider = match target.kind {
        OperationKind::ContentGeneration(kind) => return content_target(kind, model, stream),
        OperationKind::Provider(provider) => provider,
    };
    match (target.operation, provider) {
        (Operation::ListModels, P::OpenAi | P::Claude) => RequestTarget::get("/v1/models"),
        (Operation::ListModels, P::Gemini) => RequestTarget::get("/v1beta/models"),
        (Operation::GetModel, P::OpenAi | P::Claude) => {
            RequestTarget::get(format!("/v1/models/{model}"))
        }
        (Operation::GetModel, P::Gemini) => RequestTarget::get(format!("/v1beta/models/{model}")),
        (Operation::CountTokens, P::OpenAi) => RequestTarget::post("/v1/responses/input_tokens"),
        (Operation::CountTokens, P::Claude) => RequestTarget::post("/v1/messages/count_tokens"),
        (Operation::CountTokens, P::Gemini) => {
            RequestTarget::post(format!("/v1beta/models/{model}:countTokens"))
        }
        // Claude has no embeddings endpoint (no transform pair targets it);
        // the OpenAI-shaped path is a harmless placeholder.
        (Operation::CreateEmbedding, P::OpenAi | P::Claude) => {
            RequestTarget::post("/v1/embeddings")
        }
        // single-embed form; batch (`:batchEmbedContents`) is a separate op
        (Operation::CreateEmbedding, P::Gemini) => {
            RequestTarget::post(format!("/v1beta/models/{model}:embedContent"))
        }
        // OpenAI-only families: cross-op routing targets carry a
        // content-generation kind and take the content arm above, so other
        // providers never reach these rows; the OpenAI path is the one target.
        (Operation::CreateImage, _) => RequestTarget::post("/v1/images/generations"),
        (Operation::EditImage, _) => RequestTarget::post("/v1/images/edits"),
        (Operation::CompactContent, _) => RequestTarget::post("/v1/responses/compact"),
        (Operation::CreateConversation, _) => RequestTarget::post("/v1/conversations"),
        // Content ops never carry a bare Provider kind (constructor
        // invariant); synthesize the provider's content path defensively.
        (Operation::GenerateContent | Operation::StreamGenerateContent, p) => {
            let kind = match p {
                P::OpenAi => ContentGenerationKind::OpenAiResponses,
                P::Claude => ContentGenerationKind::ClaudeMessages,
                P::Gemini => ContentGenerationKind::GeminiGenerateContent,
            };
            content_target(kind, model, stream)
        }
    }
}

/// Content-generation targets (POST; gemini selects the verb by `stream`).
fn content_target(kind: ContentGenerationKind, model: &str, stream: bool) -> RequestTarget {
    use ContentGenerationKind as K;
    match kind {
        K::OpenAiChatCompletions => RequestTarget::post("/v1/chat/completions"),
        K::OpenAiResponses => RequestTarget::post("/v1/responses"),
        K::ClaudeMessages => RequestTarget::post("/v1/messages"),
        K::GeminiGenerateContent => {
            let verb = if stream {
                "streamGenerateContent"
            } else {
                "generateContent"
            };
            RequestTarget {
                method: HttpMethod::Post,
                path: format!("/v1beta/models/{model}:{verb}"),
                query: stream.then(|| "alt=sse".to_owned()),
            }
        }
    }
}
