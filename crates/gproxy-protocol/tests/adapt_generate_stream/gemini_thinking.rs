use super::all_pairs::{access, request, target};
use super::*;
use gproxy_protocol::{
    adapt::generate::{claude_gemini::ClaudeViaGemini, stream::ClaudeViaGeminiStreamFacts},
    transform::generate::claude_gemini::stream::GeminiToClaudeContext,
};

fn chunk(feed: &Feed, parts: Value) {
    let body = json!({"responseId":"gemini:source","modelVersion":"selected","candidates":[{"index":0,"content":{"role":"model","parts":parts}}]});
    feed.push(format!("data: {body}\n\n"));
}

/// A live Code Assist Claude stream: an empty text part, the thought text in
/// pieces, an empty thought part with only the signature, the call, and an
/// empty closing text part.
#[test]
fn streamed_thought_run_is_carried_merged_and_replays_as_one_native_part() {
    let store = Arc::new(Store::default());
    let state = access(&store, Dialect::Gemini);
    let mut call = ready(ClaudeViaGemini::prepare_stream(
        serde_json::from_value(request(Dialect::Claude)).unwrap(),
        target(Dialect::Claude, Dialect::Gemini),
        ClaudeViaGeminiStreamFacts {
            request: Default::default(),
            response: GeminiToClaudeContext {
                usage: Some(
                    serde_json::from_value(json!({"input_tokens":3,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens_details":{"thinking_tokens":0}})).unwrap(),
                ),
                ..Default::default()
            },
        },
        settings(),
        &state,
    ))
    .unwrap();
    let feed = Feed::default();
    chunk(&feed, json!([{"text":""}]));
    chunk(&feed, json!([{"thought":true,"text":"Let me "}]));
    chunk(&feed, json!([{"thought":true,"text":"check."}]));
    chunk(
        &feed,
        json!([{"thought":true,"thoughtSignature":"native-gemini-signature"}]),
    );
    chunk(
        &feed,
        json!([{"functionCall":{"id":"tool:source","name":"f","args":{"x":1}}}]),
    );
    let last = json!({"candidates":[{"index":0,"content":{"role":"model","parts":[{"text":""}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":2,"totalTokenCount":5,"cachedContentTokenCount":0,"thoughtsTokenCount":0,"toolUsePromptTokenCount":0}});
    feed.push(format!("data: {last}\n\n"));
    feed.close();
    let host = Host::stream(store.clone(), feed);
    ready(call.start(&host, &(), &state)).unwrap();
    let mut bytes = Vec::new();
    while let Some(event) = ready(call.next(&state)).unwrap() {
        bytes.extend_from_slice(&event.bytes);
    }
    let sse = String::from_utf8(bytes).unwrap();
    assert!(sse.contains("thinking_delta"), "{sse}");
    assert!(sse.contains("signature_delta"), "{sse}");
    assert!(sse.contains("gemini:native-gemini-signature"), "{sse}");

    let content = serde_json::to_value(call.client_result().unwrap()).unwrap()["content"].clone();
    assert_eq!(content.as_array().unwrap().len(), 2, "{content}");
    assert_eq!(content[0]["thinking"], "Let me check.");
    // The run's text and its signature travel together to the client, and
    // nothing is stored for the next turn.
    assert_eq!(content[0]["signature"], "gemini:native-gemini-signature");
    assert!(
        store
            .entries
            .lock()
            .unwrap()
            .keys()
            .all(|key| !key.starts_with("generate:"))
    );

    let tool = content[1]["id"].clone();
    let mut next = request(Dialect::Claude);
    next["stream"] = json!(false);
    next["messages"] = json!([
        {"role":"user","content":"hi"},
        {"role":"assistant","content":content},
        {"role":"user","content":[{"type":"tool_result","tool_use_id":tool,"content":"done"}]}
    ]);
    let empty = Store::default();
    let prepared = ready(ClaudeViaGemini::prepare_with_state(
        serde_json::from_value(next).unwrap(),
        target(Dialect::Claude, Dialect::Gemini).endpoint,
        target(Dialect::Claude, Dialect::Gemini).identities,
        &access(&empty, Dialect::Gemini),
        Default::default(),
    ))
    .unwrap();
    let native = serde_json::to_value(prepared.target_request()).unwrap();
    assert_eq!(
        native["contents"][1]["parts"][0],
        json!({"thought":true,"text":"Let me check.","thoughtSignature":"native-gemini-signature"})
    );
}
