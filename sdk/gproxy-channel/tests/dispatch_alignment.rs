use gproxy_channel::channel::Channel;
use gproxy_channel::channels::{
    aistudio::AiStudioChannel, anthropic::AnthropicChannel, antigravity::AntigravityChannel,
    claudecode::ClaudeCodeChannel, codex::CodexChannel, deepseek::DeepSeekChannel,
    geminicli::GeminiCliChannel, groq::GroqChannel, nvidia::NvidiaChannel,
    openrouter::OpenRouterChannel, vertex::VertexChannel, vertexexpress::VertexExpressChannel,
};
use gproxy_channel::dispatch::{RouteImplementation, RouteKey};
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

fn assert_passthrough(
    table: &gproxy_channel::DispatchTable,
    operation: OperationFamily,
    protocol: ProtocolKind,
) {
    let route = table
        .resolve(&RouteKey::new(operation, protocol))
        .expect("route exists");
    assert!(matches!(route, RouteImplementation::Passthrough));
}

fn assert_transform_to(
    table: &gproxy_channel::DispatchTable,
    src_operation: OperationFamily,
    src_protocol: ProtocolKind,
    dst_operation: OperationFamily,
    dst_protocol: ProtocolKind,
) {
    let route = table
        .resolve(&RouteKey::new(src_operation, src_protocol))
        .expect("route exists");
    assert!(matches!(
        route,
        RouteImplementation::TransformTo { destination }
            if destination.operation == dst_operation && destination.protocol == dst_protocol
    ));
}

fn assert_local(
    table: &gproxy_channel::DispatchTable,
    operation: OperationFamily,
    protocol: ProtocolKind,
) {
    let route = table
        .resolve(&RouteKey::new(operation, protocol))
        .expect("route exists");
    assert!(matches!(route, RouteImplementation::Local));
}

#[test]
fn anthropic_keeps_native_chat_completions() {
    let table = AnthropicChannel.dispatch_table();
    assert_passthrough(
        &table,
        OperationFamily::GenerateContent,
        ProtocolKind::OpenAiChatCompletion,
    );
    assert_passthrough(
        &table,
        OperationFamily::StreamGenerateContent,
        ProtocolKind::OpenAiChatCompletion,
    );
    assert_passthrough(&table, OperationFamily::ModelList, ProtocolKind::OpenAi);
    assert_passthrough(&table, OperationFamily::ModelGet, ProtocolKind::OpenAi);
}

#[test]
fn claudecode_does_not_keep_native_chat_completions() {
    let table = ClaudeCodeChannel.dispatch_table();
    assert_transform_to(
        &table,
        OperationFamily::GenerateContent,
        ProtocolKind::OpenAiChatCompletion,
        OperationFamily::GenerateContent,
        ProtocolKind::Claude,
    );
    assert_transform_to(
        &table,
        OperationFamily::StreamGenerateContent,
        ProtocolKind::OpenAiChatCompletion,
        OperationFamily::StreamGenerateContent,
        ProtocolKind::Claude,
    );
}

#[test]
fn deepseek_keeps_native_claude_and_rejects_responses_native() {
    let table = DeepSeekChannel.dispatch_table();
    assert_passthrough(
        &table,
        OperationFamily::GenerateContent,
        ProtocolKind::Claude,
    );
    assert_passthrough(
        &table,
        OperationFamily::StreamGenerateContent,
        ProtocolKind::Claude,
    );
    assert_transform_to(
        &table,
        OperationFamily::GenerateContent,
        ProtocolKind::OpenAiResponse,
        OperationFamily::GenerateContent,
        ProtocolKind::OpenAiChatCompletion,
    );
}

#[test]
fn aistudio_and_vertex_keep_native_chat_completions() {
    for table in [
        AiStudioChannel.dispatch_table(),
        VertexChannel.dispatch_table(),
    ] {
        assert_passthrough(
            &table,
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        );
        assert_passthrough(
            &table,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        );
    }
}

#[test]
fn aistudio_websocket_maps_to_gemini_live() {
    let table = AiStudioChannel.dispatch_table();
    assert_transform_to(
        &table,
        OperationFamily::OpenAiResponseWebSocket,
        ProtocolKind::OpenAi,
        OperationFamily::GeminiLive,
        ProtocolKind::Gemini,
    );
}

#[test]
fn geminicli_and_antigravity_gemini_live_use_stream_generate_content() {
    for table in [
        GeminiCliChannel.dispatch_table(),
        AntigravityChannel.dispatch_table(),
    ] {
        assert_transform_to(
            &table,
            OperationFamily::GeminiLive,
            ProtocolKind::Gemini,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini,
        );
    }
}

#[test]
fn vertexexpress_uses_gemini_generate_for_openai_images() {
    let table = VertexExpressChannel.dispatch_table();
    assert_transform_to(
        &table,
        OperationFamily::CreateImage,
        ProtocolKind::OpenAi,
        OperationFamily::GenerateContent,
        ProtocolKind::Gemini,
    );
    assert_transform_to(
        &table,
        OperationFamily::StreamCreateImage,
        ProtocolKind::OpenAi,
        OperationFamily::StreamGenerateContent,
        ProtocolKind::Gemini,
    );
}

#[test]
fn codex_groq_nvidia_and_deepseek_use_local_count_tokens() {
    for table in [
        CodexChannel.dispatch_table(),
        GroqChannel.dispatch_table(),
        NvidiaChannel.dispatch_table(),
        DeepSeekChannel.dispatch_table(),
        OpenRouterChannel.dispatch_table(),
    ] {
        assert_local(&table, OperationFamily::CountToken, ProtocolKind::OpenAi);
        assert_local(&table, OperationFamily::CountToken, ProtocolKind::Claude);
        assert_local(&table, OperationFamily::CountToken, ProtocolKind::Gemini);
    }
}
