use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind};

pub(super) struct Target {
    pub path: String,
    pub endpoint: &'static str,
}

pub(super) fn target(ctx: &PrepareCtx<'_>) -> Result<Target, ChannelError> {
    let model = crate::shared::gemini::model::model_id(ctx.upstream_model);
    if model.is_empty() {
        return Err(ChannelError::Prepare(
            "Vertex Express request has no model".into(),
        ));
    }
    let encoded = crate::shared::http::encode_component(model);
    let (verb, endpoint) = if ctx.key.operation() == Operation::CountTokens {
        ("countTokens", "gemini_count_tokens")
    } else if ctx.key.operation() == Operation::StreamGenerateContent {
        ("streamGenerateContent", "gemini_stream_generate_content")
    } else if ctx.key.operation() == Operation::GenerateContent {
        ("generateContent", "gemini_generate_content")
    } else {
        return Err(ChannelError::Prepare(
            "operation is unsupported by Vertex Express".into(),
        ));
    };
    Ok(Target {
        path: format!("/v1/publishers/google/models/{encoded}:{verb}"),
        endpoint,
    })
}

pub(super) fn is_gemini_content(ctx: &gproxy_channel_api::ResponseShapeCtx<'_>) -> bool {
    ctx.key.kind() == OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent)
        && matches!(
            ctx.key.operation(),
            Operation::GenerateContent | Operation::StreamGenerateContent
        )
}
