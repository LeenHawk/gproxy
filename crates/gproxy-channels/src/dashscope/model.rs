use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind, WireFamily};

pub(super) fn request_body(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    if super::image::is_operation(ctx.key.operation) {
        return super::image::request(ctx.key.operation, ctx.body, ctx.upstream_model);
    }
    if matches!(ctx.key.kind, OperationKind::Family(_))
        && matches!(
            ctx.key.operation,
            Operation::ListModels | Operation::GetModel
        )
    {
        return Ok(ctx.body.clone());
    }
    let model = required_model(ctx.upstream_model)?;
    match ctx.key.kind {
        OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat) => {
            let mut request: gproxy_protocol::openai::ChatCompletionRequest =
                serde_json::from_slice(ctx.body).map_err(json_error)?;
            request.model = model.into();
            encode(&request)
        }
        OperationKind::ContentGeneration(ContentGenerationKind::OpenAiResponses) => {
            let mut request: gproxy_protocol::openai::ResponseCreateRequest =
                serde_json::from_slice(ctx.body).map_err(json_error)?;
            request.model = Some(model.into());
            encode(&request)
        }
        OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) => {
            let mut request: gproxy_protocol::claude::CreateMessageRequestBody =
                serde_json::from_slice(ctx.body).map_err(json_error)?;
            request.model = model.to_owned().into();
            encode(&request)
        }
        OperationKind::Family(WireFamily::OpenAi) => match ctx.key.operation {
            Operation::CreateEmbedding => {
                let mut request: gproxy_protocol::openai::embeddings::CreateEmbeddingRequest =
                    serde_json::from_slice(ctx.body).map_err(json_error)?;
                request.model = model.into();
                encode(&request)
            }
            Operation::Rerank => {
                let mut request: gproxy_protocol::openai::rerank::RerankRequest =
                    serde_json::from_slice(ctx.body).map_err(json_error)?;
                request.model = model.into();
                encode(&request)
            }
            _ => unsupported(),
        },
        OperationKind::ContentGeneration(
            ContentGenerationKind::OpenAiResponsesWebSocket
            | ContentGenerationKind::GeminiGenerateContent,
        )
        | OperationKind::Family(WireFamily::Claude | WireFamily::Gemini) => unsupported(),
    }
}

pub(super) fn is_claude(key: gproxy_protocol::OperationKey) -> bool {
    key.kind == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
}

fn required_model(model: &str) -> Result<&str, ChannelError> {
    (!model.trim().is_empty())
        .then_some(model.trim())
        .ok_or_else(|| ChannelError::Prepare("DashScope request has no model".into()))
}

fn encode(value: &impl serde::Serialize) -> Result<Bytes, ChannelError> {
    serde_json::to_vec(value)
        .map(Bytes::from)
        .map_err(json_error)
}

fn unsupported() -> Result<Bytes, ChannelError> {
    Err(ChannelError::Prepare(
        "operation is unsupported by DashScope model rewriting".into(),
    ))
}

fn json_error(error: serde_json::Error) -> ChannelError {
    ChannelError::Prepare(format!("DashScope request JSON: {error}"))
}
