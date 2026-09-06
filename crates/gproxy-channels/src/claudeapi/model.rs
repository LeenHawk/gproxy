use gproxy_channel_api::ChannelError;
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind, WireFamily};
use http::Method;

pub(super) fn target(
    key: OperationKey,
    model: &str,
) -> Result<(&'static Method, String), ChannelError> {
    if is_models(key, Operation::ListModels) {
        Ok((&Method::GET, "/v1/models".into()))
    } else if is_models(key, Operation::GetModel) {
        Ok((
            &Method::GET,
            format!(
                "/v1/models/{}",
                crate::shared::http::encode_component(model)
            ),
        ))
    } else if key == family(Operation::CountTokens) {
        Ok((&Method::POST, "/v1/messages/count_tokens".into()))
    } else if is_messages(key) {
        Ok((&Method::POST, "/v1/messages".into()))
    } else if is_chat(key) {
        Ok((&Method::POST, "/v1/chat/completions".into()))
    } else {
        Err(ChannelError::Prepare(
            "operation is unsupported by Claude API".into(),
        ))
    }
}

pub(super) fn endpoint_name(key: OperationKey) -> Option<&'static str> {
    gproxy_channel_api::endpoint_override_key(key)
}

fn is_models(key: OperationKey, operation: Operation) -> bool {
    key.operation() == operation
        && matches!(
            key.kind(),
            OperationKind::Family(WireFamily::OpenAi | WireFamily::Claude)
        )
}

pub(super) fn is_chat(key: OperationKey) -> bool {
    key.kind() == OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat)
        && matches!(
            key.operation(),
            Operation::GenerateContent | Operation::StreamGenerateContent
        )
}

pub(super) fn is_messages(key: OperationKey) -> bool {
    key.kind() == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
        && matches!(
            key.operation(),
            Operation::GenerateContent | Operation::StreamGenerateContent
        )
}

pub(super) fn is_count_tokens(key: OperationKey) -> bool {
    key == family(Operation::CountTokens)
}

const fn family(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::Claude)
}
