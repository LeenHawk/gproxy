use super::*;
#[test]
fn truncated_chat_results_restore_real_function_or_custom_output_kind() {
    for custom in [false, true] {
        let call = if custom {
            json!({"type":"custom_tool_call","id":"ctc-native","call_id":"native-call","name":"grammar","input":"native input"})
        } else {
            json!({"type":"function_call","id":"fc-native","call_id":"native-call","name":"lookup","arguments":"{}"})
        };
        let mut body = output("r");
        body["output"] = json!([call]);
        let host = Host::new(body);
        let store = Store::default();
        let state = state(&store, Dialect::OpenAi);
        let mut progress = GenerationProgress::default();
        let mut p = ChatViaResponses::prepare(
            serde_json::from_value(input("h")).unwrap(),
            "selected",
            endpoint(),
            ids(Dialect::OpenAiChat, Dialect::OpenAi),
        )
        .unwrap();
        ready(p.invoke(
            &host,
            &(),
            codec_limits(),
            &state,
            &mut progress,
            |_| Ok(()),
        ))
        .unwrap();
        let mut next = input("h");
        next["messages"] =
            json!([{"role":"tool","tool_call_id":"native-call","content":"actual result"}]);
        let p = ready(ChatViaResponses::prepare_with_state(
            serde_json::from_value(next).unwrap(),
            "selected",
            endpoint(),
            ids(Dialect::OpenAiChat, Dialect::OpenAi),
            &state,
        ))
        .unwrap();
        let target = serde_json::to_value(p.target_request()).unwrap();
        assert_eq!(target["input"].as_array().unwrap().len(), 1);
        assert_eq!(
            target["input"][0]["type"],
            if custom {
                "custom_tool_call_output"
            } else {
                "function_call_output"
            }
        );
        assert_eq!(target["input"][0]["call_id"], "native-call");
        assert_eq!(target["input"][0]["output"], "actual result");
    }
}
#[test]
fn declared_call_kind_conflicting_with_saved_kind_is_rejected() {
    let mut body = output("r");
    body["output"] = json!([{"type":"function_call","id":"fc-native","call_id":"native-call","name":"lookup","arguments":"{}"}]);
    let host = Host::new(body);
    let store = Store::default();
    let state = state(&store, Dialect::OpenAi);
    let mut progress = GenerationProgress::default();
    let mut p = ChatViaResponses::prepare(
        serde_json::from_value(input("h")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::OpenAiChat, Dialect::OpenAi),
    )
    .unwrap();
    ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &state,
        &mut progress,
        |_| Ok(()),
    ))
    .unwrap();
    let mut next = input("h");
    next["messages"] = json!([{"role":"assistant","tool_calls":[{"id":"native-call","type":"custom","custom":{"name":"lookup","input":"{}"}}]},{"role":"tool","tool_call_id":"native-call","content":"result"}]);
    let error = ready(ChatViaResponses::prepare_with_state(
        serde_json::from_value(next).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::OpenAiChat, Dialect::OpenAi),
        &state,
    ))
    .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::InvalidInput);
}
