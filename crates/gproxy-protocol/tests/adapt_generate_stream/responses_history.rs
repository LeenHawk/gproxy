use super::*;
use gproxy_protocol::adapt::generate::chat_responses::ResponsesViaChat;

/// Streams one Responses turn over a Chat upstream that answers every request
/// with the same `chatcmpl-mock` ID, as mocks and many OpenAI-compatible
/// servers do, and returns the client's final response.
fn turn(store: &Arc<Store>, request: Value, namespace: u8) -> Value {
    let access = all_pairs::access(store, Dialect::OpenAiChat);
    let mut target = all_pairs::target(Dialect::OpenAi, Dialect::OpenAiChat);
    // Core gives every invocation fresh random namespaces; distinct constants
    // stand in for them here.
    target.identities = GenerationIdentity::new(
        IdNamespace([namespace; 16]),
        IdNamespace([namespace + 1; 16]),
        Dialect::OpenAi,
        Dialect::OpenAiChat,
    )
    .unwrap();
    let mut call = ready(ResponsesViaChat::prepare_stream(
        serde_json::from_value(request).unwrap(),
        target,
        all_pairs::chat_response_context(),
        settings(),
        &access,
    ))
    .unwrap();
    let sent: Value = serde_json::to_value(call.target_request()).unwrap();
    let asked = sent["messages"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .to_string();
    let mut body = all_pairs::native_response(Dialect::OpenAiChat);
    body["id"] = json!("chatcmpl-mock");
    body["choices"][0]["message"]["content"] = json!(format!("answer to {asked}"));
    body["choices"][0]["message"]["tool_calls"] = Value::Null;
    body["choices"][0]["finish_reason"] = json!("stop");
    let feed = Feed::default();
    for event in gproxy_protocol::transform::generate::stream::chat::synthesize_chat_stream(
        serde_json::from_value(body).unwrap(),
        Default::default(),
    )
    .unwrap()
    .value
    {
        feed.push(format!(
            "data: {}\n\n",
            serde_json::to_string(&event).unwrap()
        ));
    }
    feed.push("data: [DONE]\n\n");
    feed.close();
    let host = Host::stream(store.clone(), feed);
    ready(call.start(&host, &(), &access)).unwrap();
    while ready(call.next(&access)).unwrap().is_some() {}
    serde_json::to_value(call.client_result().unwrap()).unwrap()
}

fn first(question: &str) -> Value {
    json!({"model":"client","stream":true,"input":question})
}

fn next(previous: &Value, question: &str) -> Value {
    json!({"model":"client","stream":true,"input":question,"previous_response_id":previous["id"]})
}

/// Upstream IDs used to key continuation history. Two conversations answered
/// with the same `chatcmpl-mock` ID collided: the second write was refused,
/// and a later turn could read the other conversation's history. The gateway
/// now names each stored response itself.
#[test]
fn repeated_upstream_response_ids_keep_each_conversations_history() {
    let store = Arc::new(Store::default());
    let alpha = turn(&store, first("alpha question"), 10);
    let beta = turn(&store, first("beta question"), 20);
    for response in [&alpha, &beta] {
        let id = response["id"].as_str().unwrap();
        assert!(id.starts_with("resp_"), "{id}");
        assert_ne!(id, "chatcmpl-mock");
    }
    assert_ne!(alpha["id"], beta["id"]);
    assert_eq!(
        store
            .entries
            .lock()
            .unwrap()
            .keys()
            .filter(|key| key.starts_with("responses-history:"))
            .count(),
        2
    );

    let access = all_pairs::access(&store, Dialect::OpenAiChat);
    for (previous, own, other) in [(&alpha, "alpha", "beta"), (&beta, "beta", "alpha")] {
        let continued = ready(ResponsesViaChat::prepare_stream(
            serde_json::from_value(next(previous, "and then?")).unwrap(),
            all_pairs::target(Dialect::OpenAi, Dialect::OpenAiChat),
            all_pairs::chat_response_context(),
            settings(),
            &access,
        ))
        .unwrap();
        let messages =
            serde_json::to_value(continued.target_request()).unwrap()["messages"].to_string();
        assert!(messages.contains(&format!("{own} question")), "{messages}");
        assert!(messages.contains("answer to"), "{messages}");
        assert!(!messages.contains(other), "{messages}");
        assert!(messages.contains("and then?"), "{messages}");
    }

    // A continued turn is stored under its own new ID too, so it chains.
    let alpha2 = turn(&store, next(&alpha, "alpha follow-up"), 30);
    assert_ne!(alpha2["id"], alpha["id"]);
    let continued = ready(ResponsesViaChat::prepare_stream(
        serde_json::from_value(next(&alpha2, "last")).unwrap(),
        all_pairs::target(Dialect::OpenAi, Dialect::OpenAiChat),
        all_pairs::chat_response_context(),
        settings(),
        &access,
    ))
    .unwrap();
    let messages =
        serde_json::to_value(continued.target_request()).unwrap()["messages"].to_string();
    assert!(messages.contains("alpha question"), "{messages}");
    assert!(messages.contains("alpha follow-up"), "{messages}");
    assert!(!messages.contains("beta"), "{messages}");
}

/// A `store=false` turn writes nothing, so it keeps the upstream's own ID.
#[test]
fn unstored_turn_keeps_the_upstream_response_id() {
    let store = Arc::new(Store::default());
    let mut request = first("quiet");
    request["store"] = json!(false);
    let response = turn(&store, request, 40);
    assert_eq!(response["id"], "chatcmpl-mock");
    assert!(store.entries.lock().unwrap().is_empty());
}
