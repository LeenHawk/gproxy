use bytes::Bytes;
use gproxy_channel_api::{ChannelError, ResponseShapeCtx};
use gproxy_protocol::gemini::VeoOperation;
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind};

pub(super) fn shape(ctx: ResponseShapeCtx<'_>) -> Result<Bytes, ChannelError> {
    if !ctx.status.is_success() {
        return Ok(ctx.body.clone());
    }
    match ctx.key.operation() {
        Operation::CreateEmbedding | Operation::BatchCreateEmbedding => {
            super::embeddings::response(ctx.body, ctx.key.operation())
        }
        Operation::ListModels => crate::shared::gemini::vertex::normalize_model(ctx.body, true),
        Operation::GetModel => crate::shared::gemini::vertex::normalize_model(ctx.body, false),
        Operation::GenerateContent | Operation::StreamGenerateContent
            if ctx.key.kind()
                == OperationKind::ContentGeneration(
                    ContentGenerationKind::GeminiGenerateContent,
                ) =>
        {
            crate::shared::gemini::vertex::normalize_content(ctx.body)
        }
        Operation::CreateVideo | Operation::RetrieveVideo => video(ctx.body),
        _ => Ok(ctx.body.clone()),
    }
}

fn video(body: &Bytes) -> Result<Bytes, ChannelError> {
    let mut operation: VeoOperation = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Observe(format!("Vertex video response JSON: {error}")))?;
    if let Some(name) = operation.name.as_deref() {
        operation.rest.insert(
            "vertexOperationName".into(),
            serde_json::Value::String(name.into()),
        );
        operation.name = Some(format!(
            "operations/{}",
            super::resource::encode_operation(name)
        ));
    }
    serde_json::to_vec(&operation)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Observe(error.to_string()))
}
