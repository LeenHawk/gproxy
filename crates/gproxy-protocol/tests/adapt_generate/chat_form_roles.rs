use super::*;
use gproxy_protocol::{
    transform::identity::{IdentityRole, OutputItemKind},
    wire::openai::responses as r,
};
fn check_roles(
    access: &GenerationStateAccess<'_, Store>,
    result: GenerationOutcome<r::GenerateContentResponseBody>,
    native_response: &str,
) {
    let GenerationOutcome::Success { response, .. } = result else {
        panic!("rejected")
    };
    let mut saw_call = false;
    for item in response.body.output {
        let (role, id, is_call) = match item {
            r::ResponseOutputItem::FunctionCall(v) => (
                IdentityRole::OutputItem(OutputItemKind::FunctionCall),
                v.id.unwrap(),
                true,
            ),
            r::ResponseOutputItem::Message(v) => (
                IdentityRole::OutputItem(OutputItemKind::Message),
                v.id,
                false,
            ),
            _ => continue,
        };
        let saved = ready(access.read(role, &id)).unwrap().unwrap();
        assert!(
            saved.original_item_id.is_none(),
            "source call/response IDs are not original item IDs"
        );
        assert_eq!(
            saved.original_call_id.as_deref(),
            is_call.then_some("actual-call")
        );
        assert_eq!(saved.response_id.as_deref(), Some(native_response));
        saw_call |= is_call;
    }
    assert!(saw_call);
}
#[test]
fn native_chat_tool_and_message_keep_their_source_identity_roles_in_responses_state() {
    let store = Store::default();
    let access = state(&store, Dialect::OpenAiChat);
    let mut body = output("h");
    body["choices"][0]["message"]["tool_calls"] = json!([{"type":"function","id":"actual-call","function":{"name":"lookup","arguments":"{}"}}]);
    body["choices"][0]["finish_reason"] = json!("tool_calls");
    body["usage"]["prompt_tokens_details"]["cache_write_tokens"] = json!(0);
    let host = Host::new(body);
    let mut p = ResponsesViaChat::prepare(
        serde_json::from_value(input("r")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::OpenAi, Dialect::OpenAiChat),
    )
    .unwrap();
    let result = ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &access,
        &mut GenerationProgress::default(),
        |_| {
            Ok(ChatReturnFacts {
                parallel_tool_calls: true,
                tool_choice: auto(),
                prompt_cache_options: None,
                usage: Default::default(),
            })
        },
    ))
    .unwrap();
    check_roles(&access, result, "chat-native");
}
#[test]
fn native_claude_tool_keeps_call_role_in_responses_item_state() {
    let store = Store::default();
    let access = state(&store, Dialect::Claude);
    let mut body = output("c");
    body["content"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"tool_use","id":"actual-call","name":"lookup","input":{}}));
    body["stop_reason"] = json!("tool_use");
    let host = Host::new(body);
    let mut p = ResponsesViaClaude::prepare(
        serde_json::from_value(input("r")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::OpenAi, Dialect::Claude),
        Default::default(),
    )
    .unwrap();
    let result = ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &access,
        &mut GenerationProgress::default(),
        |_| {
            Ok(ClaudeReturnFacts {
                parallel_tool_calls: true,
                tool_choice: auto(),
                prompt_cache_options: None,
                usage: Default::default(),
                created_at: 123,
            })
        },
    ))
    .unwrap();
    check_roles(&access, result, "msg-native");
}
#[test]
fn native_gemini_tool_keeps_call_role_in_responses_item_state() {
    let store = Store::default();
    let access = state(&store, Dialect::Gemini);
    let mut body = output("g");
    body["candidates"][0]["content"]["parts"]
        .as_array_mut()
        .unwrap()
        .push(json!({"functionCall":{"id":"actual-call","name":"lookup","args":{}}}));
    let host = Host::new(body);
    let mut p = ResponsesViaGemini::prepare(
        serde_json::from_value(input("r")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::OpenAi, Dialect::Gemini),
        Default::default(),
    )
    .unwrap();
    let result = ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &access,
        &mut GenerationProgress::default(),
        |_| {
            Ok(GeminiReturnFacts {
                parallel_tool_calls: true,
                tool_choice: auto(),
                prompt_cache_options: None,
                usage: gproxy_protocol::transform::generate::gemini_responses::GeminiUsageFacts {
                    cache_write_tokens: Some(0),
                    cached_tokens: None,
                },
                created_at: 123,
            })
        },
    ))
    .unwrap();
    check_roles(&access, result, "g-native");
}
