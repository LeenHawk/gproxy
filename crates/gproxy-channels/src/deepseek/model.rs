use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::{ContentGenerationKind, OperationKind};

pub(super) fn rewrite(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    let model = ctx.upstream_model.trim();
    if model.is_empty() {
        return Err(ChannelError::Prepare(
            "DeepSeek content request has no model".into(),
        ));
    }
    match ctx.key.kind() {
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
        OperationKind::ContentGeneration(
            ContentGenerationKind::OpenAiResponsesWebSocket
            | ContentGenerationKind::GeminiGenerateContent,
        )
        | OperationKind::Family(_) => Err(ChannelError::Prepare(
            "operation is unsupported by DeepSeek model rewriting".into(),
        )),
    }
}

pub(super) fn is_chat(key: gproxy_protocol::OperationKey) -> bool {
    key.kind() == OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat)
}

pub(super) fn is_responses(key: gproxy_protocol::OperationKey) -> bool {
    key.kind() == OperationKind::ContentGeneration(ContentGenerationKind::OpenAiResponses)
}

pub(super) fn is_claude(key: gproxy_protocol::OperationKey) -> bool {
    key.kind() == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
}

fn encode(value: &impl serde::Serialize) -> Result<Bytes, ChannelError> {
    serde_json::to_vec(value)
        .map(Bytes::from)
        .map_err(json_error)
}

fn json_error(error: serde_json::Error) -> ChannelError {
    ChannelError::Prepare(format!("DeepSeek request JSON: {error}"))
}
