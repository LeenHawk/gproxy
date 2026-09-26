use super::*;
#[test]
fn chat_results_take_the_function_or_custom_kind_from_declared_history() {
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
        // Responses' own call ID reached the client unchanged, so nothing
        // was stored, and the call's kind comes from the client's history.
        assert!(store.entries.lock().unwrap().is_empty());
        let declared = if custom {
            json!({"id":"native-call","type":"custom","custom":{"name":"grammar","input":"native input"}})
        } else {
            json!({"id":"native-call","type":"function","function":{"name":"lookup","arguments":"{}"}})
        };
        let mut next = input("h");
        next["messages"] = json!([
            {"role":"assistant","tool_calls":[declared]},
            {"role":"tool","tool_call_id":"native-call","content":"actual result"}
        ]);
        let p = ready(ChatViaResponses::prepare_with_state(
            serde_json::from_value(next).unwrap(),
            endpoint(),
            ids(Dialect::OpenAiChat, Dialect::OpenAi),
            &state,
        ))
        .unwrap();
        let target = serde_json::to_value(p.target_request()).unwrap();
        let output = &target["input"][1];
        assert_eq!(
            output["type"],
            if custom {
                "custom_tool_call_output"
            } else {
                "function_call_output"
            }
        );
        assert_eq!(output["call_id"], "native-call");
        assert_eq!(output["output"], "actual result");
        // A result without its call has no kind to replay: Chat itself
        // requires the declaring assistant message.
        let mut truncated = input("h");
        truncated["messages"] =
            json!([{"role":"tool","tool_call_id":"native-call","content":"actual result"}]);
        assert_eq!(
            ready(ChatViaResponses::prepare_with_state(
                serde_json::from_value(truncated).unwrap(),
                endpoint(),
                ids(Dialect::OpenAiChat, Dialect::OpenAi),
                &state,
            ))
            .unwrap_err()
            .kind(),
            TransformErrorKind::MissingMetadata
        );
    }
}
