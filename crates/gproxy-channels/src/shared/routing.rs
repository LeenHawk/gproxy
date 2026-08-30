use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};

pub const fn openai(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::OpenAi)
}

pub const fn claude(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::Claude)
}

pub const fn gemini(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::Gemini)
}

pub const fn openai_chat(operation: Operation) -> OperationKey {
    OperationKey::content(operation, ContentGenerationKind::OpenAiChat)
}

pub const fn openai_responses(operation: Operation) -> OperationKey {
    OperationKey::content(operation, ContentGenerationKind::OpenAiResponses)
}

pub const fn openai_responses_websocket(operation: Operation) -> OperationKey {
    OperationKey::content(operation, ContentGenerationKind::OpenAiResponsesWebSocket)
}

pub const fn claude_messages(operation: Operation) -> OperationKey {
    OperationKey::content(operation, ContentGenerationKind::ClaudeMessages)
}

pub const fn gemini_generate_content(operation: Operation) -> OperationKey {
    OperationKey::content(operation, ContentGenerationKind::GeminiGenerateContent)
}

macro_rules! route {
    (pass $operation:ident, $kind:ident) => {
        gproxy_channel_api::ChannelSupport::passthrough($crate::shared::routing::$kind(
            gproxy_protocol::Operation::$operation,
        ))
    };
    (xform $operation:ident, $kind:ident => $target_operation:ident, $target_kind:ident) => {
        gproxy_channel_api::ChannelSupport::transform(
            $crate::shared::routing::$kind(gproxy_protocol::Operation::$operation),
            $crate::shared::routing::$target_kind(gproxy_protocol::Operation::$target_operation),
        )
    };
    (local $operation:ident, $kind:ident) => {
        gproxy_channel_api::ChannelSupport::local($crate::shared::routing::$kind(
            gproxy_protocol::Operation::$operation,
        ))
    };
    (unsupported $operation:ident, $kind:ident) => {
        gproxy_channel_api::ChannelSupport::unsupported($crate::shared::routing::$kind(
            gproxy_protocol::Operation::$operation,
        ))
    };
}

pub(crate) use route;
