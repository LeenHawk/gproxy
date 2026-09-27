//! Upstream signatures travel to the client in its own protocol's field and
//! come back from there: every next turn here runs against an empty store.

use super::*;

fn claude_gemini_facts(
    _: &gproxy_protocol::wire::gemini::GenerateContentResponseBody,
) -> Result<claude_gemini::ClaudeGeminiUsageFacts, TransformError> {
    Ok(claude_gemini::ClaudeGeminiUsageFacts {
        cache_creation_input_tokens: Some(0),
        cache_read_input_tokens: Some(0),
        thinking_tokens: None,
    })
}

fn responses_claude_facts(
    _: &gproxy_protocol::wire::claude::generate_content::GenerateContentResponseBody,
) -> Result<ClaudeReturnFacts, TransformError> {
    Ok(ClaudeReturnFacts {
        parallel_tool_calls: true,
        tool_choice: auto(),
        prompt_cache_options: None,
        usage: Default::default(),
        created_at: 123,
    })
}

fn responses_gemini_facts(
    _: &gproxy_protocol::wire::gemini::GenerateContentResponseBody,
) -> Result<GeminiReturnFacts, TransformError> {
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
}

fn no_generate_state(store: &Store) {
    assert!(
        store
            .entries
            .lock()
            .unwrap()
            .keys()
            .all(|key| !key.starts_with("generate:")),
        "a signature was stored"
    );
}

/// The first Claude turn against a Gemini upstream answering `parts`.
fn claude_turn(parts: Value) -> Value {
    let mut body = output("g");
    body["candidates"][0]["content"]["parts"] = parts;
    let host = Host::new(body);
    let store = Store::default();
    let mut prepared = ClaudeViaGemini::prepare(
        serde_json::from_value(input("c")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::Claude, Dialect::Gemini),
        Default::default(),
    )
    .unwrap();
    let GenerationOutcome::Success { response, .. } = ready(prepared.invoke(
        &host,
        &(),
        codec_limits(),
        &state(&store, Dialect::Gemini),
        &mut GenerationProgress::default(),
        claude_gemini_facts,
    ))
    .unwrap() else {
        panic!("rejected")
    };
    no_generate_state(&store);
    serde_json::to_value(response.body).unwrap()["content"].clone()
}

/// The Gemini request for a next Claude turn replaying `content`, prepared
/// against a fresh, empty store.
fn claude_next(content: &Value, args: Option<Value>) -> Value {
    let mut content = content.clone();
    let call = content
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|block| block["type"] == "tool_use")
        .unwrap();
    if let Some(args) = args {
        call["input"] = args;
    }
    let id = call["id"].clone();
    let mut next = input("c");
    next["tools"] = json!([{"name":"Edit","input_schema":{
        "type":"object","properties":{"x":{"type":"integer"},"replace_all":{"type":"boolean","default":false}},"required":["x"]
    }}]);
    next["messages"] = json!([
        {"role":"user","content":"hi"},
        {"role":"assistant","content":content},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":id,"content":"done"}]}
    ]);
    let store = Store::default();
    let prepared = ready(ClaudeViaGemini::prepare_with_state(
        serde_json::from_value(next).unwrap(),
        endpoint(),
        ids(Dialect::Claude, Dialect::Gemini),
        &state(&store, Dialect::Gemini),
        Default::default(),
    ))
    .unwrap();
    no_generate_state(&store);
    serde_json::to_value(prepared.target_request()).unwrap()
}

#[test]
fn claude_client_carries_a_flash_call_signature_to_the_next_turn() {
    // Gemini flash: unsigned thought text, then the signature on the call.
    let content = claude_turn(json!([
        {"thought":true,"text":"plan"},
        {"functionCall":{"id":"native-call","name":"Edit","args":{"x":1}},"thoughtSignature":"flash-signature"}
    ]));
    assert_eq!(
        content,
        json!([
            {"type":"thinking","thinking":"plan","signature":"gemini:"},
            {"type":"thinking","thinking":"","signature":"gemini-next:flash-signature"},
            {"type":"tool_use","id":"native-call","name":"Edit","input":{"x":1}}
        ])
    );
    // Claude Code adds schema defaults to the arguments it sends back; the
    // call is rebuilt from what the client sends, with the carried signature.
    let target = claude_next(&content, Some(json!({"x":1,"replace_all":false})));
    assert_eq!(
        target["contents"][1]["parts"],
        json!([{
            "functionCall":{"id":"native-call","name":"Edit","args":{"x":1,"replace_all":false}},
            "thoughtSignature":"flash-signature"
        }])
    );
    assert_eq!(
        target["contents"][2]["parts"][0]["functionResponse"]["name"],
        "Edit"
    );
}

#[test]
fn claude_client_carries_an_antigravity_claude_thought_run_to_the_next_turn() {
    // Claude behind Antigravity: the thought text first, then an empty
    // thought part with only the signature, then the call.
    let content = claude_turn(json!([
        {"thought":true,"text":"Let me "},
        {"thought":true,"text":"check."},
        {"thought":true,"thoughtSignature":"claude-behind-antigravity"},
        {"functionCall":{"id":"toolu_1","name":"Edit","args":{"x":1}}}
    ]));
    assert_eq!(
        content[0],
        json!({"type":"thinking","thinking":"Let me check.","signature":"gemini:claude-behind-antigravity"})
    );
    assert_eq!(content[1]["type"], "tool_use");
    // The signature must sit on the thought part with the whole text: an
    // empty-text thought part or a signed call is refused or ignored there.
    let target = claude_next(&content, None);
    assert_eq!(
        target["contents"][1]["parts"],
        json!([
            {"thought":true,"text":"Let me check.","thoughtSignature":"claude-behind-antigravity"},
            {"functionCall":{"id":"toolu_1","name":"Edit","args":{"x":1}}}
        ])
    );
}

#[test]
fn claude_client_gemini_signatures_never_reach_another_upstream() {
    let content = claude_turn(json!([
        {"thought":true,"text":"Let me check."},
        {"thought":true,"thoughtSignature":"gemini-signature"},
        {"functionCall":{"id":"toolu_1","name":"Edit","args":{"x":1}},"thoughtSignature":"call-signature"}
    ]));
    let mut next = input("c");
    next["messages"] = json!([
        {"role":"user","content":"hi"},
        {"role":"assistant","content":content},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"done"}]}
    ]);
    let next = || serde_json::from_value(next.clone()).unwrap();
    let responses = ClaudeViaResponses::prepare(
        next(),
        "selected",
        endpoint(),
        ids(Dialect::Claude, Dialect::OpenAi),
    )
    .unwrap();
    let chat = ClaudeViaChat::prepare(
        next(),
        "selected",
        endpoint(),
        ids(Dialect::Claude, Dialect::OpenAiChat),
    )
    .unwrap();
    for target in [
        serde_json::to_value(responses.target_request()).unwrap(),
        serde_json::to_value(chat.target_request()).unwrap(),
    ] {
        let raw = target.to_string();
        assert!(!raw.contains("signature"), "{raw}");
        assert!(!raw.contains("Let me check."), "{raw}");
    }
    // Sent to Anthropic as it is, the body loses the Gemini-signed thinking.
    let body = serde_json::to_vec(&json!({"model":"claude","max_tokens":8,"messages":[
        {"role":"user","content":"hi"},
        {"role":"assistant","content":content},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"done"}]}
    ]}))
    .unwrap();
    let stripped: Value =
        serde_json::from_slice(&claude_gemini::without_gemini_thinking(&body).unwrap()).unwrap();
    assert_eq!(
        stripped["messages"][1]["content"],
        json!([{"type":"tool_use","id":"toolu_1","name":"Edit","input":{"x":1}}])
    );
}

#[test]
fn responses_client_carries_claude_thinking_to_the_next_turn() {
    let mut body = output("c");
    body["content"] = json!([
        {"type":"thinking","thinking":"private reasoning","signature":"native-claude-signature"},
        {"type":"tool_use","id":"toolu_1","name":"lookup","input":{"x":1}}
    ]);
    body["stop_reason"] = json!("tool_use");
    let host = Host::new(body);
    let store = Store::default();
    let mut p = ResponsesViaClaude::prepare(
        serde_json::from_value(input("r")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::OpenAi, Dialect::Claude),
        Default::default(),
    )
    .unwrap();
    let GenerationOutcome::Success { response, .. } = ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &state(&store, Dialect::Claude),
        &mut GenerationProgress::default(),
        responses_claude_facts,
    ))
    .unwrap() else {
        panic!("rejected")
    };
    no_generate_state(&store);
    let wire = serde_json::to_value(response.body).unwrap();
    assert_eq!(
        wire["output"][0]["encrypted_content"],
        "claude:native-claude-signature"
    );
    let mut items = wire["output"].as_array().unwrap().clone();
    items.push(json!({"type":"function_call_output","call_id":"toolu_1","output":"result"}));
    let mut next = input("r");
    next["input"] = json!(items);
    let store = Store::default();
    let restored = ready(ResponsesViaClaude::prepare_with_state(
        serde_json::from_value(next.clone()).unwrap(),
        endpoint(),
        ids(Dialect::OpenAi, Dialect::Claude),
        &state(&store, Dialect::Claude),
        Default::default(),
    ))
    .unwrap();
    no_generate_state(&store);
    let sent = serde_json::to_value(restored.target_request()).unwrap();
    assert_eq!(
        sent["messages"][0]["content"][0],
        json!({"type":"thinking","thinking":"private reasoning","signature":"native-claude-signature"})
    );
    let raw = sent.to_string();
    assert!(raw.contains("\"tool_use\""), "{raw}");
    assert!(raw.contains("\"tool_result\""), "{raw}");
    assert!(!raw.contains("claude:"), "{raw}");
}

#[test]
fn responses_client_model_switch_strips_a_gemini_signature_before_claude() {
    let mut next = input("r");
    next["input"] = json!([
        {"type":"message","role":"user","content":"hi"},
        {"type":"reasoning","id":"rs_1","summary":[],"content":[{"type":"reasoning_text","text":"gemini thought"}],"encrypted_content":"gemini:gemini-signature"},
        {"type":"reasoning","id":"rs_2","summary":[],"encrypted_content":"gemini-next:call-signature"},
        {"type":"function_call","call_id":"call_1","name":"lookup","arguments":"{}"},
        {"type":"function_call_output","call_id":"call_1","output":"result"}
    ]);
    let store = Store::default();
    let prepared = ready(ResponsesViaClaude::prepare_with_state(
        serde_json::from_value(next.clone()).unwrap(),
        endpoint(),
        ids(Dialect::OpenAi, Dialect::Claude),
        &state(&store, Dialect::Claude),
        Default::default(),
    ))
    .unwrap();
    let raw = serde_json::to_value(prepared.target_request())
        .unwrap()
        .to_string();
    assert!(!raw.contains("thinking"), "{raw}");
    assert!(!raw.contains("signature"), "{raw}");
    assert!(raw.contains("\"tool_use\""), "{raw}");
    // Sent to a Responses upstream as it is, the carried reasoning goes too.
    let stripped: Value = serde_json::from_slice(
        &gproxy_protocol::transform::generate::signature::without_carried_reasoning(
            &serde_json::to_vec(&next).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(stripped["input"].as_array().unwrap().len(), 3);
    assert!(!stripped.to_string().contains("signature"));
}

#[test]
fn responses_client_carries_gemini_thoughts_and_call_signatures_to_the_next_turn() {
    let mut body = output("g");
    body["candidates"][0]["content"]["parts"] = json!([
        {"thought":true,"text":"Let me "},
        {"thought":true,"text":"check."},
        {"thought":true,"thoughtSignature":"thought-signature"},
        {"functionCall":{"id":"native-call","name":"lookup","args":{"x":1}},"thoughtSignature":"call-signature"}
    ]);
    let host = Host::new(body);
    let store = Store::default();
    let mut p = ResponsesViaGemini::prepare(
        serde_json::from_value(input("r")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::OpenAi, Dialect::Gemini),
        Default::default(),
    )
    .unwrap();
    let GenerationOutcome::Success { response, .. } = ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &state(&store, Dialect::Gemini),
        &mut GenerationProgress::default(),
        responses_gemini_facts,
    ))
    .unwrap() else {
        panic!("rejected")
    };
    no_generate_state(&store);
    let wire = serde_json::to_value(response.body).unwrap();
    let output = wire["output"].as_array().unwrap();
    let carried: Vec<_> = output
        .iter()
        .map(|item| item["encrypted_content"].clone())
        .collect();
    assert_eq!(
        carried,
        [
            Value::Null,
            Value::Null,
            json!("gemini:thought-signature"),
            json!("gemini-next:call-signature"),
            Value::Null
        ]
    );
    assert_eq!(output[4]["type"], "function_call");
    let mut items = output.clone();
    items.push(
        json!({"type":"function_call_output","call_id":"native-call","output":"actual result"}),
    );
    let mut next = input("r");
    next["input"] = json!(items);
    let store = Store::default();
    let restored = ready(ResponsesViaGemini::prepare_with_state(
        serde_json::from_value(next).unwrap(),
        endpoint(),
        ids(Dialect::OpenAi, Dialect::Gemini),
        &state(&store, Dialect::Gemini),
        Default::default(),
    ))
    .unwrap();
    no_generate_state(&store);
    let target = serde_json::to_value(restored.target_request()).unwrap();
    assert_eq!(
        target["contents"][0]["parts"],
        json!([{"thought":true,"text":"Let me check.","thoughtSignature":"thought-signature"}])
    );
    assert_eq!(
        target["contents"][1]["parts"],
        json!([{"functionCall":{"id":"native-call","name":"lookup","args":{"x":1}},"thoughtSignature":"call-signature"}])
    );
    assert_eq!(
        target["contents"][2]["parts"][0]["functionResponse"]["name"],
        "lookup"
    );
}

#[test]
fn chat_client_carries_gemini_signatures_in_reasoning_details() {
    let mut body = output("g");
    body["candidates"][0]["content"]["parts"] = json!([
        {"thought":true,"text":"Let me "},
        {"thought":true,"text":"check.","thoughtSignature":"thought-signature"},
        {"functionCall":{"name":"lookup","args":{"x":1}},"thoughtSignature":"call-signature"}
    ]);
    let host = Host::new(body);
    let store = Store::default();
    let mut p = chat_gemini();
    let GenerationOutcome::Success { response, .. } = ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &state(&store, Dialect::Gemini),
        &mut GenerationProgress::default(),
        gemini_facts,
    ))
    .unwrap() else {
        panic!("rejected")
    };
    no_generate_state(&store);
    let response = serde_json::to_value(response.body).unwrap();
    let message = &response["choices"][0]["message"];
    let call = message["tool_calls"][0].clone();
    assert_eq!(message["reasoning_content"], "Let me check.");
    assert_eq!(
        message["reasoning_details"],
        json!([
            {"type":"reasoning.text","format":"google-gemini-v1","index":0,"text":"Let me check.","signature":"thought-signature"},
            {"type":"reasoning.encrypted","format":"google-gemini-v1","index":1,"id":call["id"],"data":"call-signature"}
        ])
    );
    let mut assistant = message.clone();
    assistant["tool_calls"][0]["function"]["arguments"] = json!("{\"x\":2}");
    let mut next = input("h");
    next["messages"] =
        json!([assistant, {"role":"tool","tool_call_id":call["id"],"content":"actual result"}]);
    let store = Store::default();
    let prepared = ready(ChatViaGemini::prepare_with_state(
        serde_json::from_value(next).unwrap(),
        endpoint(),
        ids(Dialect::OpenAiChat, Dialect::Gemini),
        &state(&store, Dialect::Gemini),
        &Default::default(),
    ))
    .unwrap();
    no_generate_state(&store);
    let target = serde_json::to_value(prepared.target_request()).unwrap();
    let parts = &target["contents"][0]["parts"];
    assert_eq!(
        parts[0],
        json!({"thought":true,"text":"Let me check.","thoughtSignature":"thought-signature"})
    );
    // The call is rebuilt from the client's history, so changed arguments go
    // to the upstream with the signature; the upstream decides.
    assert_eq!(parts[1]["thoughtSignature"], "call-signature");
    assert_eq!(parts[1]["functionCall"]["args"], json!({"x":2}));
    assert_eq!(parts.as_array().unwrap().len(), 2);
}
