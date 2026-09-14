use super::*;
use gproxy_protocol::adapt::generate::chat_claude::ClaudeViaChat;

#[test]
fn tool_reference_history_reaches_the_streaming_target_request() {
    let store = Store::default();
    let mut access = state(&store);
    access.target = IdentityTarget::new("selected", Dialect::OpenAiChat)
        .unwrap()
        .with_origin("origin")
        .unwrap();
    let mut target = selected();
    target.identities = GenerationIdentity::new(
        IdNamespace([71; 16]),
        IdNamespace([72; 16]),
        Dialect::Claude,
        Dialect::OpenAiChat,
    )
    .unwrap();
    let prepared = ready(ClaudeViaChat::prepare_stream(
        serde_json::from_value(json!({"model":"source","stream":true,"max_tokens":32,
            "tools":[{"name":"lookup","input_schema":{"type":"object","properties":{}}}],
            "messages":[
                {"role":"assistant","content":[{"type":"tool_use","id":"call-search","name":"discover","input":{}}]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"call-search","content":[{"type":"tool_reference","tool_name":"lookup"}]}]}
            ]})).unwrap(),
        target, Default::default(), settings(), &access,
    )).unwrap();
    let wire = serde_json::to_value(prepared.target_request()).unwrap();
    assert_eq!(wire["stream"], true);
    let reference: Value =
        serde_json::from_str(wire["messages"][1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(
        reference,
        json!({"type":"tool_reference","tool_name":"lookup"})
    );
    assert_eq!(
        wire["messages"][1]["tool_call_id"],
        wire["messages"][0]["tool_calls"][0]["id"]
    );
}
