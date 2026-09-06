mod video;

use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind, WireFamily};
use serde_json::Value;

const ANTHROPIC_VERSION: &str = "vertex-2023-10-16";

pub(super) fn request(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    if super::embeddings::uses_predict(ctx) {
        return super::embeddings::request(ctx);
    }
    if ctx.key.operation() == Operation::CreateEmbedding {
        return super::embeddings::single_request(ctx.body);
    }
    if ctx.key.operation() == Operation::CreateVideo {
        return video::create(ctx.body);
    }
    if ctx.key.operation() == Operation::RetrieveVideo {
        let operation = super::resource::request_operation(ctx.path)?;
        return serde_json::to_vec(&serde_json::json!({"operationName": operation}))
            .map(Bytes::from)
            .map_err(json_error);
    }
    if is_claude(ctx) {
        return claude(ctx);
    }
    if ctx.key.kind() == OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat) {
        let model = if let Some(publisher) = ctx.upstream_model.strip_prefix("publishers/") {
            publisher.replace("/models/", "/")
        } else if ctx.upstream_model.contains('/') {
            ctx.upstream_model.to_owned()
        } else {
            format!("google/{}", ctx.upstream_model)
        };
        return crate::shared::openai::shape_request(
            ctx.key,
            ctx.stream,
            &model,
            ctx.headers,
            ctx.body,
        );
    }
    crate::shared::gemini::model::rewrite(ctx.key.operation(), ctx.body, ctx.upstream_model)
}

fn claude(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    let mut body = crate::shared::claude::hygiene::json_object(ctx.body)?;
    let root = body.as_object_mut().expect("JSON object was validated");
    root.entry("anthropic_version")
        .or_insert_with(|| Value::String(ANTHROPIC_VERSION.into()));
    if ctx.key.kind() == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) {
        root.remove("model");
    } else if ctx.key.kind() == OperationKind::Family(WireFamily::Claude)
        && !ctx.upstream_model.is_empty()
    {
        root.insert(
            "model".into(),
            Value::String(super::model::model_id(ctx.upstream_model).into()),
        );
    }
    serde_json::to_vec(&body)
        .map(Bytes::from)
        .map_err(json_error)
}

fn is_claude(ctx: &PrepareCtx<'_>) -> bool {
    super::model::is_claude(ctx.key)
        && matches!(
            ctx.key.operation(),
            Operation::CountTokens | Operation::GenerateContent | Operation::StreamGenerateContent
        )
}

fn json_error(error: serde_json::Error) -> ChannelError {
    ChannelError::Prepare(format!("Vertex request JSON: {error}"))
}
