//! A call the upstream sent without an ID, or with one the client's dialect
//! rejects, reaches the client under an alias that names it. The next turn
//! replays it from the alias and the client's own history alone: nothing is
//! written in between.
use super::*;

fn claude_facts(
    _: &gproxy_protocol::wire::gemini::GenerateContentResponseBody,
) -> Result<claude_gemini::ClaudeGeminiUsageFacts, TransformError> {
    Ok(claude_gemini::ClaudeGeminiUsageFacts {
        cache_creation_input_tokens: Some(0),
        cache_read_input_tokens: Some(0),
        thinking_tokens: Some(0),
    })
}

fn parts(request: &Value, field: &str) -> Vec<Value> {
    request["contents"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| c["parts"].as_array().unwrap().clone())
        .filter_map(|p| p.get(field).cloned())
        .collect()
}

#[test]
fn claude_client_replays_gemini_calls_without_state() {
    let store = Store::default();
    let access = state(&store, Dialect::Gemini);
    let mut body = output("g");
    body["candidates"][0]["content"]["parts"] = json!([
        {"functionCall":{"name":"lookup","args":{"q":1}}},
        {"functionCall":{"id":"a.b","name":"lookup","args":{"q":2}}}
    ]);
    let host = Host::new(body);
    let mut p = ClaudeViaGemini::prepare(
        serde_json::from_value(input("c")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::Claude, Dialect::Gemini),
        Default::default(),
    )
    .unwrap();
    let GenerationOutcome::Success { response, .. } = ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &access,
        &mut GenerationProgress::default(),
        claude_facts,
    ))
    .unwrap() else {
        panic!("rejected")
    };
    let content = serde_json::to_value(response.body).unwrap()["content"].clone();
    let uses: Vec<Value> = content
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["type"] == "tool_use")
        .cloned()
        .collect();
    let aliases: Vec<&str> = uses.iter().map(|b| b["id"].as_str().unwrap()).collect();
    // Claude's alphabet rejects `a.b`, so it is escaped; the ID-less call is
    // marked as such and numbered by its position in the response.
    assert_eq!(aliases, ["toolu_gpn_0202020202020202_0", "toolu_gpe_a_2eb"]);
    assert!(store.entries.lock().unwrap().is_empty());

    let mut next = input("c");
    next["messages"] = json!([
        {"role":"user","content":"hi"},
        {"role":"assistant","content":uses},
        {"role":"user","content":[
            {"type":"tool_result","tool_use_id":aliases[0],"content":"one"},
            {"type":"tool_result","tool_use_id":aliases[1],"content":"two"}
        ]}
    ]);
    let mut p = ready(ClaudeViaGemini::prepare_with_state(
        serde_json::from_value(next).unwrap(),
        endpoint(),
        ids(Dialect::Claude, Dialect::Gemini),
        &access,
        Default::default(),
    ))
    .unwrap();
    let target = serde_json::to_value(p.target_request()).unwrap();
    let calls = parts(&target, "functionCall");
    let results = parts(&target, "functionResponse");
    // The escaped call goes back under Gemini's own ID. The ID-less one keeps
    // its alias, an ID Gemini takes as opaque; its name, from the client's
    // tool_use, pairs the result with the call.
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0]["id"], aliases[0]);
    assert_eq!(calls[1]["id"], "a.b");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["id"], aliases[0]);
    assert_eq!(results[1]["id"], "a.b");
    for part in calls.iter().chain(&results) {
        assert_eq!(part["name"], "lookup");
    }
    let next_host = Host::new(output("g"));
    ready(p.invoke(
        &next_host,
        &(),
        codec_limits(),
        &access,
        &mut GenerationProgress::default(),
        claude_facts,
    ))
    .unwrap();
    assert_eq!(next_host.sent.lock().unwrap().len(), 1);
    assert!(store.entries.lock().unwrap().is_empty());
}

#[test]
fn gemini_client_replays_a_legacy_chat_call_without_state() {
    let store = Store::default();
    let access = state(&store, Dialect::OpenAiChat);
    let mut body = output("h");
    body["choices"][0]["message"]["function_call"] =
        json!({"name":"lookup","arguments":"{\"x\":1}"});
    body["choices"][0]["finish_reason"] = json!("function_call");
    let host = Host::new(body);
    let mut p = GeminiViaChat::prepare(
        serde_json::from_value(input("g")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::Gemini, Dialect::OpenAiChat),
    )
    .unwrap();
    let GenerationOutcome::Success { response, .. } = ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &access,
        &mut GenerationProgress::default(),
        |_| Ok(()),
    ))
    .unwrap() else {
        panic!("rejected")
    };
    let response = serde_json::to_value(response.body).unwrap();
    let call = parts(
        &json!({"contents":[response["candidates"][0]["content"]]}),
        "functionCall",
    )
    .remove(0);
    assert_eq!(call["id"], "call_gpl_0202020202020202_0");
    assert!(store.entries.lock().unwrap().is_empty());

    let mut next = input("g");
    next["contents"] = json!([
        {"role":"user","parts":[{"text":"hi"}]},
        {"role":"model","parts":[{"functionCall":call}]},
        {"role":"user","parts":[{"functionResponse":{"id":call["id"],"name":"lookup","response":{"actual":"result"}}}]}
    ]);
    let mut p = ready(GeminiViaChat::prepare_with_state(
        serde_json::from_value(next).unwrap(),
        endpoint(),
        ids(Dialect::Gemini, Dialect::OpenAiChat),
        &access,
    ))
    .unwrap();
    let target = serde_json::to_value(p.target_request()).unwrap();
    // The alias says the call was a legacy function_call, so it goes back in
    // that form, named from the client's own history.
    let messages = target["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[1]["function_call"]["name"], "lookup");
    assert!(messages[1].get("tool_calls").is_none());
    assert_eq!(messages[2]["role"], "function");
    assert_eq!(messages[2]["name"], "lookup");
    let next_host = Host::new(output("h"));
    ready(p.invoke(
        &next_host,
        &(),
        codec_limits(),
        &access,
        &mut GenerationProgress::default(),
        |_| Ok(()),
    ))
    .unwrap();
    assert_eq!(next_host.sent.lock().unwrap().len(), 1);
    // Neither turn wrote or read any identity state.
    assert!(store.entries.lock().unwrap().is_empty());
    assert_eq!(*store.reads.lock().unwrap(), 0);
}
