use super::*;
use gproxy_protocol::{
    adapt::generate::{
        claude_responses::ResponsesViaClaude, gemini_responses::ResponsesViaGemini,
        stream::bridge::StreamBridge,
    },
    transform::generate::stream as native,
    wire::openai::responses::stream as rs,
};
#[path = "../support/client_tools_backends.rs"]
mod fixtures;

fn run<B: StreamBridge<ClientEvent = rs::StreamEvent>>(
    mut call: StreamInvocation<B>,
    source: Vec<String>,
    backend: Dialect,
    store: &Arc<Store>,
) -> Value {
    let access = all_pairs::access(store, backend);
    let feed = Feed::default();
    for event in source {
        feed.push(format!("data: {event}\n\n"));
    }
    feed.close();
    let host = Host::stream(store.clone(), feed);
    ready(call.start(&host, &(), &access)).unwrap();
    let mut observed = 0;
    while let Some(chunk) = ready(call.next(&access)).unwrap() {
        for line in std::str::from_utf8(&chunk.bytes).unwrap().lines() {
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            let event: Value = serde_json::from_str(data).unwrap();
            if let Some(id) = event["item"]["call_id"].as_str() {
                let record = ready(access.read(IdentityRole::ToolCall, id))
                    .unwrap()
                    .expect("tool alias must be durable before its event is yielded");
                assert!(
                    record
                        .original_call_id
                        .as_deref()
                        .unwrap()
                        .starts_with("native.")
                );
                observed += 1;
            }
        }
    }
    assert_eq!(observed, 8);
    assert_eq!(host.sent.lock().unwrap().len(), 1);
    serde_json::to_value(call.client_result().unwrap()).unwrap()
}

#[test]
fn claude_client_tools_stream_persist_and_resume_from_previous_response() {
    let store = Arc::new(Store::default());
    let access = all_pairs::access(&store, Dialect::Claude);
    let call = ready(ResponsesViaClaude::prepare_stream(
        serde_json::from_value(fixtures::request(true)).unwrap(),
        all_pairs::target(Dialect::OpenAi, Dialect::Claude),
        ResponsesViaClaudeStreamFacts {
            request: Default::default(),
            response: all_pairs::claude_response_context(),
        },
        settings(),
        &access,
    ))
    .unwrap();
    let request = serde_json::to_value(call.target_request()).unwrap();
    let names: Vec<_> = request["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect();
    let mut events = native::claude::synthesize_claude_stream(
        serde_json::from_value(fixtures::claude(&names)).unwrap(),
        Default::default(),
    )
    .unwrap()
    .value;
    // Start every call before finishing any: later blocks must wait behind an
    // unfinished native action without losing the Responses output order.
    use gproxy_protocol::wire::claude::stream::StreamEvent as C;
    events.sort_by_key(|event| match event {
        C::MessageStart(_) => (0, 0),
        C::ContentBlockStart(v) => (1, v.index),
        C::ContentBlockDelta(v) => (2, -v.index),
        C::ContentBlockStop(v) => (3, -v.index),
        C::MessageDelta(_) => (4, 0),
        C::MessageStop(_) => (5, 0),
        _ => (6, 0),
    });
    let events = events
        .into_iter()
        .map(|e| serde_json::to_string(&e).unwrap())
        .collect();
    let output = run(call, events, Dialect::Claude, &store);
    let mut followup = fixtures::followup(&output);
    followup["stream"] = json!(true);
    followup["previous_response_id"] = output["id"].clone();
    followup["input"].as_array_mut().unwrap().drain(..4);
    let mut target = all_pairs::target(Dialect::OpenAi, Dialect::Claude);
    target.identities = GenerationIdentity::new(
        IdNamespace([101; 16]),
        IdNamespace([102; 16]),
        Dialect::OpenAi,
        Dialect::Claude,
    )
    .unwrap();
    let next = ready(ResponsesViaClaude::prepare_stream(
        serde_json::from_value(followup).unwrap(),
        target,
        ResponsesViaClaudeStreamFacts {
            request: Default::default(),
            response: all_pairs::claude_response_context(),
        },
        settings(),
        &access,
    ))
    .unwrap();
    let next = serde_json::to_value(next.target_request()).unwrap();
    assert_eq!(next["tools"].as_array().unwrap().len(), 5);
    assert!(next["messages"].to_string().contains("native.0"));
    assert!(next["messages"].to_string().contains("context mismatch"));
}

#[test]
fn gemini_client_tools_preserve_signed_parts_and_resume_discovery_history() {
    let store = Arc::new(Store::default());
    let access = all_pairs::access(&store, Dialect::Gemini);
    let call = ready(ResponsesViaGemini::prepare_stream(
        serde_json::from_value(fixtures::request(true)).unwrap(),
        all_pairs::target(Dialect::OpenAi, Dialect::Gemini),
        ResponsesViaGeminiStreamFacts {
            request: Default::default(),
            response: all_pairs::gemini_response_context(),
        },
        settings(),
        &access,
    ))
    .unwrap();
    let request = serde_json::to_value(call.target_request()).unwrap();
    let names: Vec<_> = request["tools"][0]["functionDeclarations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect();
    let source = fixtures::gemini(&names, true);
    let events = native::gemini::synthesize_gemini_stream(
        serde_json::from_value(source.clone()).unwrap(),
        Default::default(),
    )
    .unwrap()
    .value
    .into_iter()
    .map(|e| serde_json::to_string(&e).unwrap())
    .collect();
    let output = run(call, events, Dialect::Gemini, &store);
    let followup = fixtures::followup(&output);
    let next = ready(ResponsesViaGemini::prepare_with_state(
        serde_json::from_value(followup.clone()).unwrap(),
        Endpoint::new("/generate").unwrap(),
        GenerationIdentity::new(
            IdNamespace([103; 16]),
            IdNamespace([104; 16]),
            Dialect::OpenAi,
            Dialect::Gemini,
        )
        .unwrap(),
        &access,
        Default::default(),
    ))
    .unwrap();
    let next = serde_json::to_value(next.target_request()).unwrap();
    let calls: Vec<_> = next["contents"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| c["parts"].as_array().into_iter().flatten())
        .filter(|p| p.get("functionCall").is_some())
        .cloned()
        .collect();
    assert_eq!(
        calls,
        source["candidates"][0]["content"]["parts"]
            .as_array()
            .unwrap()
            .clone()
    );
    let mut tampered = followup.clone();
    tampered["input"][0]["action"]["commands"] = json!(["changed"]);
    assert!(
        ready(ResponsesViaGemini::prepare_with_state(
            serde_json::from_value(tampered).unwrap(),
            Endpoint::new("/generate").unwrap(),
            GenerationIdentity::new(
                IdNamespace([105; 16]),
                IdNamespace([106; 16]),
                Dialect::OpenAi,
                Dialect::Gemini
            )
            .unwrap(),
            &access,
            Default::default(),
        ))
        .is_err()
    );
    let mut followup = followup;
    followup["stream"] = json!(true);
    followup["previous_response_id"] = output["id"].clone();
    followup["input"].as_array_mut().unwrap().drain(..4);
    let mut target = all_pairs::target(Dialect::OpenAi, Dialect::Gemini);
    target.identities = GenerationIdentity::new(
        IdNamespace([107; 16]),
        IdNamespace([108; 16]),
        Dialect::OpenAi,
        Dialect::Gemini,
    )
    .unwrap();
    let resumed = ready(ResponsesViaGemini::prepare_stream(
        serde_json::from_value(followup).unwrap(),
        target,
        ResponsesViaGeminiStreamFacts {
            request: Default::default(),
            response: all_pairs::gemini_response_context(),
        },
        settings(),
        &access,
    ))
    .unwrap();
    let resumed = serde_json::to_value(resumed.target_request()).unwrap();
    assert_eq!(
        resumed["tools"][0]["functionDeclarations"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert!(resumed["contents"].to_string().contains("c2lnbmF0dXJl"));
}
