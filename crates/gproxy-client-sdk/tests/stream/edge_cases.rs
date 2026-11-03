//! Stream decoding edge cases found in review.
use super::*;
use serde_json::Value;

#[cfg(feature = "claude")]
#[test]
fn claude_tool_without_arguments_streams_an_empty_json_delta() {
    let native = json!({
        "type":"message","id":"msg_1","model":"claude","role":"assistant",
        "stop_reason":"tool_use","stop_sequence":null,
        "content":[{"type":"tool_use","id":"toolu_1","name":"now","input":{}}],
        "usage":{"input_tokens":1,"output_tokens":1}
    });
    let parsed = dialects::claude::parse(serde_json::from_value(native.clone()).unwrap()).unwrap();
    let mut events = support::claude_events(&native);
    // Claude sends `partial_json: ""` for a tool with no arguments.
    for event in &mut events {
        if event["delta"]["type"] == "input_json_delta" {
            event["delta"]["partial_json"] = json!("");
        }
    }
    let streamed = support::aggregate(support::decode(Dialect::Claude, events));
    assert_eq!(streamed.content, parsed.content);
    let Content::ToolCall(call) = &streamed.content[0].content else {
        panic!("tool call expected");
    };
    assert_eq!(call.arguments, "{}");
}

#[cfg(feature = "gemini")]
#[test]
fn gemini_grounding_part_index_counts_aggregated_parts_after_a_thought() {
    // The thought streams as two fragments that aggregate into one part, so the
    // text is aggregated part 1 although it is the third streamed fragment.
    let thought_1 =
        json!({"candidates":[{"content":{"role":"model","parts":[{"text":"pl","thought":true}]}}]});
    let thought_2 =
        json!({"candidates":[{"content":{"role":"model","parts":[{"text":"an","thought":true}]}}]});
    let text = json!({"candidates":[{"content":{"role":"model","parts":[{"text":"grounded"}]}}]});
    let last = json!({"candidates":[{"finishReason":"STOP","groundingMetadata":{
        "groundingChunks":[{"web":{"uri":"https://example.com/source","title":"Source"}}],
        "groundingSupports":[{"segment":{"partIndex":1,"startIndex":0,"endIndex":8,"text":"grounded"},"groundingChunkIndices":[0]}]
    }}]});
    let mut combined = json!({"candidates":[{"content":{"role":"model","parts":[
        {"text":"plan","thought":true},{"text":"grounded"}
    ]}}]});
    combined["candidates"][0]["groundingMetadata"] =
        last["candidates"][0]["groundingMetadata"].clone();
    combined["candidates"][0]["finishReason"] = json!("STOP");
    let parsed = dialects::gemini::parse(serde_json::from_value(combined).unwrap()).unwrap();
    let streamed = support::aggregate(support::decode(
        Dialect::Gemini,
        vec![thought_1, thought_2, text, last],
    ));
    assert_eq!(streamed.content, parsed.content);
    let Content::Text(text) = &streamed.content[1].content else {
        panic!("text expected");
    };
    assert_eq!(text.citations.len(), 1);
}

#[cfg(feature = "chat")]
#[test]
fn chat_tool_call_id_and_name_repeated_on_every_delta_are_not_concatenated() {
    let mut decoder = stream::StreamDecoder::new(Dialect::OpenaiChat);
    let mut events = Vec::new();
    for (arguments, reason) in [
        ("{\"city\":", Value::Null),
        ("\"Paris\"}", Value::Null),
        ("", json!("tool_calls")),
    ] {
        let delta = json!({"tool_calls":[{"index":0,"id":"call_1","type":"function",
            "function":{"name":"get_weather","arguments":arguments}}]});
        events.extend(
            decoder
                .push_json(
                    &json!({"id":"chat_1","object":"chat.completion.chunk",
            "created":1,"model":"m","choices":[{"index":0,"delta":delta,"finish_reason":reason}]})
                    .to_string(),
                )
                .unwrap(),
        );
    }
    events.extend(decoder.finish().unwrap());
    let starts = events
        .iter()
        .filter(|e| matches!(e, Event::ToolCallStart { .. }))
        .count();
    assert_eq!(starts, 1);
    let result = support::aggregate(events);
    let Content::ToolCall(call) = &result.content[0].content else {
        panic!("tool call expected");
    };
    assert_eq!(
        (call.id.as_str(), call.name.as_str()),
        ("call_1", "get_weather")
    );
    assert_eq!(call.arguments, "{\"city\":\"Paris\"}");
}
