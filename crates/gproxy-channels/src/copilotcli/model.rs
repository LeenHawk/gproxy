use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind};

pub(super) fn rewrite(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    if ctx.key.operation == Operation::ListModels {
        return Ok(ctx.body.clone());
    }
    if ctx.key.kind != OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat) {
        return Err(ChannelError::Prepare(
            "operation is unsupported by Copilot CLI".into(),
        ));
    }
    let model = ctx.upstream_model.trim();
    if model.is_empty() {
        return Err(ChannelError::Prepare(
            "Copilot Chat request has no model".into(),
        ));
    }
    let mut request: gproxy_protocol::openai::ChatCompletionRequest =
        serde_json::from_slice(ctx.body)
            .map_err(|error| ChannelError::Prepare(format!("Copilot Chat JSON: {error}")))?;
    request.model = model.into();
    serde_json::to_vec(&request)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}
