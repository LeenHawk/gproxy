use super::*;
use gproxy_protocol::transform::identity::{IdentityRole, IdentityTarget, OutputItemKind};

fn facts(
    _: &gproxy_protocol::wire::gemini::GenerateContentResponseBody,
) -> Result<claude_gemini::ClaudeGeminiUsageFacts, TransformError> {
    Ok(claude_gemini::ClaudeGeminiUsageFacts {
        cache_creation_input_tokens: Some(0),
        cache_read_input_tokens: Some(0),
        thinking_tokens: None,
    })
}

/// Code Assist's shape: thought text first, then an empty thought part that
/// carries only the signature, then the call.
fn signed_turn(store: &Store) -> Value {
    let mut body = output("g");
    body["candidates"][0]["content"]["parts"] = json!([
        {"thought":true,"text":"Let me "},
        {"thought":true,"text":"check."},
        {"thought":true,"thoughtSignature":"native-gemini-signature"},
        {"functionCall":{"id":"native-call","name":"lookup","args":{"x":1}}}
    ]);
    let host = Host::new(body);
    let mut prepared = ClaudeViaGemini::prepare(
        serde_json::from_value(input("c")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::Claude, Dialect::Gemini),
        Default::default(),
    )
    .unwrap();
    let result = ready(prepared.invoke(
        &host,
        &(),
        codec_limits(),
        &state(store, Dialect::Gemini),
        &mut GenerationProgress::default(),
        facts,
    ))
    .unwrap();
    let GenerationOutcome::Success { response, .. } = result else {
        panic!("rejected")
    };
    serde_json::to_value(response.body).unwrap()["content"].clone()
}

/// The next turn replaying `content` (the assistant turn as the client got it).
fn next_turn(content: &Value) -> Value {
    let call = content
        .as_array()
        .unwrap()
        .iter()
        .find(|block| block["type"] == "tool_use")
        .unwrap()["id"]
        .clone();
    let mut next = input("c");
    next["tools"] = json!([{"name":"lookup","input_schema":{"type":"object","properties":{"x":{"type":"integer"}}}}]);
    next["messages"] = json!([
        {"role":"user","content":"hi"},
        {"role":"assistant","content":content},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":call,"content":"done"}]}
    ]);
    next
}

fn replayed(next: Value, state: &GenerationStateAccess<'_, Store>) -> Value {
    let prepared = ready(ClaudeViaGemini::prepare_with_state(
        serde_json::from_value(next).unwrap(),
        endpoint(),
        ids(Dialect::Claude, Dialect::Gemini),
        state,
        Default::default(),
    ))
    .unwrap();
    serde_json::to_value(prepared.target_request()).unwrap()
}

#[test]
fn gemini_thought_run_reaches_claude_under_a_handle_and_replays_as_one_native_part() {
    let store = Store::default();
    let content = signed_turn(&store);
    assert_eq!(content[0]["type"], "thinking");
    assert_eq!(content[0]["thinking"], "Let me check.");
    let handle = content[0]["signature"].as_str().unwrap();
    assert!(claude_gemini::is_thinking_handle(handle));
    assert!(!content.to_string().contains("native-gemini-signature"));
    assert_eq!(content[1]["type"], "tool_use");

    let id = claude_gemini::thinking_handle_id(handle).unwrap();
    let state = state(&store, Dialect::Gemini);
    let record = ready(state.read(IdentityRole::OutputItem(OutputItemKind::Reasoning), id))
        .unwrap()
        .unwrap();
    assert_eq!(
        record.opaque_signature.unwrap().value,
        "native-gemini-signature"
    );

    // Text and signature travel together in one part: the upstream rejects
    // an empty-text thought and ignores a signature anywhere else.
    let target = replayed(next_turn(&content), &state);
    assert_eq!(
        target["contents"][1]["parts"][0],
        json!({"thought":true,"text":"Let me check.","thoughtSignature":"native-gemini-signature"})
    );
    assert_eq!(
        target["contents"][1]["parts"][1]["functionCall"]["name"],
        "lookup"
    );
    assert!(
        !target
            .to_string()
            .contains(claude_gemini::THINKING_HANDLE_PREFIX)
    );
}

/// Every replay that cannot restore the saved part drops the thinking block
/// and keeps the call; the handle never reaches the upstream.
fn assert_dropped(target: &Value) {
    let parts = target["contents"][1]["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 1, "{target}");
    assert_eq!(parts[0]["functionCall"]["name"], "lookup");
    assert!(
        !target
            .to_string()
            .contains(claude_gemini::THINKING_HANDLE_PREFIX)
    );
    assert!(!target.to_string().contains("native-gemini-signature"));
}

#[test]
fn a_handle_is_dropped_when_it_cannot_be_restored() {
    let store = Store::default();
    let content = signed_turn(&store);

    let mut edited = content.clone();
    edited[0]["thinking"] = json!("Let me check twice.");
    assert_dropped(&replayed(
        next_turn(&edited),
        &state(&store, Dialect::Gemini),
    ));

    let mut unsigned = content.clone();
    unsigned[0]["signature"] = json!(format!("{}unsigned", claude_gemini::THINKING_HANDLE_PREFIX));
    assert_dropped(&replayed(
        next_turn(&unsigned),
        &state(&store, Dialect::Gemini),
    ));

    let empty = Store::default();
    assert_dropped(&replayed(
        next_turn(&content),
        &state(&empty, Dialect::Gemini),
    ));

    for target in [
        IdentityTarget::new("other-model", Dialect::Gemini)
            .unwrap()
            .with_origin("actual-upstream")
            .unwrap(),
        IdentityTarget::new("selected", Dialect::Gemini)
            .unwrap()
            .with_origin("other-upstream")
            .unwrap(),
    ] {
        let mut other = state(&store, Dialect::Gemini);
        other.target = target;
        assert_dropped(&replayed(next_turn(&content), &other));
    }

    // An expired entry reads as missing state, never as an error.
    let mut expired = state(&store, Dialect::Gemini);
    expired.now = std::time::UNIX_EPOCH + Duration::from_secs(2000);
    let mut next = input("c");
    next["messages"] = json!([
        {"role":"user","content":"hi"},
        {"role":"assistant","content":[content[0].clone(),{"type":"text","text":"ok"}]},
        {"role":"user","content":"again"}
    ]);
    let target = replayed(next, &expired);
    assert_eq!(target["contents"][1]["parts"], json!([{"text":"ok"}]));
}

#[test]
fn a_handle_never_leaves_for_a_stateless_or_foreign_target() {
    let store = Store::default();
    let content = signed_turn(&store);
    let next = || serde_json::from_value(next_turn(&content)).unwrap();

    let stateless = ClaudeViaGemini::prepare(
        next(),
        "selected",
        endpoint(),
        ids(Dialect::Claude, Dialect::Gemini),
        Default::default(),
    )
    .unwrap();
    assert_dropped(&serde_json::to_value(stateless.target_request()).unwrap());

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
        assert!(
            !raw.contains(claude_gemini::THINKING_HANDLE_PREFIX),
            "{raw}"
        );
        assert!(!raw.contains("Let me check."), "{raw}");
    }
}
