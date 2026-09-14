use super::*;
use crate::client_tools_base as fixtures;
use gproxy_protocol::adapt::generate::chat_responses::ResponsesViaChat;

#[test]
fn local_tools_stream_as_native_items_and_replay_with_original_call_ids() {
    let store = Arc::new(Store::default());
    let access = all_pairs::access(&store, Dialect::OpenAiChat);
    let mut call = ready(ResponsesViaChat::prepare_stream(
        serde_json::from_value(fixtures::request(true)).unwrap(),
        all_pairs::target(Dialect::OpenAi, Dialect::OpenAiChat),
        all_pairs::chat_response_context(),
        settings(),
        &access,
    ))
    .unwrap();
    let request = serde_json::to_value(call.target_request()).unwrap();
    let names: Vec<_> = request["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap().to_owned())
        .collect();
    let source = gproxy_protocol::transform::generate::stream::chat::synthesize_chat_stream(
        serde_json::from_value(fixtures::response(&names)).unwrap(),
        Default::default(),
    )
    .unwrap()
    .value;
    let feed = Feed::default();
    for event in source {
        feed.push(format!(
            "data: {}\n\n",
            serde_json::to_string(&event).unwrap()
        ));
    }
    feed.push("data: [DONE]\n\n");
    feed.close();
    let host = Host::stream(store.clone(), feed);
    ready(call.start(&host, &(), &access)).unwrap();
    let mut wire_events = String::new();
    while let Some(chunk) = ready(call.next(&access)).unwrap() {
        wire_events.push_str(std::str::from_utf8(&chunk.bytes).unwrap());
    }
    let response = serde_json::to_value(call.client_result().unwrap()).unwrap();
    assert!(wire_events.contains("shell_call"));
    assert!(wire_events.contains("apply_patch_call"));
    assert!(wire_events.contains("tool_search_call"));
    for (index, item) in response["output"].as_array().unwrap().iter().enumerate() {
        let id = item["call_id"].as_str().unwrap();
        assert_ne!(id, format!("native.{index}"));
        let saved = ready(access.read(IdentityRole::ToolCall, id))
            .unwrap()
            .unwrap();
        assert_eq!(
            saved.original_call_id.as_deref(),
            Some(format!("native.{index}").as_str())
        );
    }
    let next = ready(ResponsesViaChat::prepare_with_state(
        serde_json::from_value(fixtures::followup(&response)).unwrap(),
        "selected",
        Endpoint::new("/chat/completions").unwrap(),
        GenerationIdentity::new(
            IdNamespace([91; 16]),
            IdNamespace([92; 16]),
            Dialect::OpenAi,
            Dialect::OpenAiChat,
        )
        .unwrap(),
        &access,
    ))
    .unwrap();
    let request = serde_json::to_value(next.target_request()).unwrap();
    let calls: Vec<_> = request["messages"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|m| m["tool_calls"].as_array().into_iter().flatten())
        .collect();
    for (index, call) in calls.iter().enumerate() {
        assert_eq!(call["id"], format!("native.{index}"));
    }
    assert_eq!(request["tools"].as_array().unwrap().len(), 5);
    let mut continuation = fixtures::followup(&response);
    continuation["stream"] = json!(true);
    continuation["previous_response_id"] = response["id"].clone();
    continuation["input"].as_array_mut().unwrap().drain(..4);
    let mut target = all_pairs::target(Dialect::OpenAi, Dialect::OpenAiChat);
    target.identities = GenerationIdentity::new(
        IdNamespace([95; 16]),
        IdNamespace([96; 16]),
        Dialect::OpenAi,
        Dialect::OpenAiChat,
    )
    .unwrap();
    let continued = ready(ResponsesViaChat::prepare_stream(
        serde_json::from_value(continuation).unwrap(),
        target,
        all_pairs::chat_response_context(),
        settings(),
        &access,
    ))
    .unwrap();
    let continued = serde_json::to_value(continued.target_request()).unwrap();
    assert_eq!(continued["tools"].as_array().unwrap().len(), 5);
    assert!(continued["messages"].to_string().contains("native.0"));
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}
