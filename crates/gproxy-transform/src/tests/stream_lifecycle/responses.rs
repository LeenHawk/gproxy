use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind as Kind, Operation};
use serde_json::{Value, json};

use super::super::content;
use super::super::support::data_frames;
use crate::ResponseStream;

#[test]
fn chat_to_responses_closes_tools_and_preserves_trailing_usage() {
    let mut stream = ResponseStream::new(
        content(Operation::StreamGenerateContent, Kind::OpenAiResponses),
        content(Operation::StreamGenerateContent, Kind::OpenAiChat),
    )
    .unwrap();
    let deltas = [
        json!({"role":"assistant","content":""}),
        json!({"reasoning_content":"Run the test.","content":""}),
        json!({"tool_calls":[
            {"index":0,"id":"shell","type":"function","function":{"name":"exec_command","arguments":"{\"cmd\":"}},
            {"index":1,"id":"patch","type":"custom","custom":{"name":"apply_patch","input":"*** Begin Patch\n"}}
        ]}),
        json!({"tool_calls":[
            {"index":1,"custom":{"input":"*** End Patch"}},
            {"index":0,"function":{"arguments":"\"python3 test_calc.py\"}"}}
        ]}),
        json!({}),
    ];
    let mut events = Vec::new();
    for (index, delta) in deltas.into_iter().enumerate() {
        let chunk = json!({"id":"chat_tools","object":"chat.completion.chunk","created":1,
            "model":"test-model","choices":[{"index":0,"delta":delta,
                "finish_reason":(index == 4).then_some("tool_calls")}]});
        let output = stream
            .push(Bytes::from(format!("data: {chunk}\n\n")))
            .unwrap();
        events.extend(data_frames(&output.concat()));
    }
    assert!(
        events
            .iter()
            .all(|event| event["type"] != "response.completed")
    );
    let usage = json!({"id":"chat_tools","object":"chat.completion.chunk","created":1,
        "model":"test-model","choices":[],"usage":{"prompt_tokens":12,"completion_tokens":7,
            "total_tokens":19,"completion_tokens_details":{"reasoning_tokens":3}}});
    let output = stream
        .push(Bytes::from(format!("data: {usage}\n\ndata: [DONE]\n\n")))
        .unwrap();
    events.extend(data_frames(&output.concat()));
    assert!(stream.finish().unwrap().is_empty());
    assert_eq!(events[0]["type"], "response.created");
    let mut open = std::collections::BTreeSet::new();
    let mut completed = Vec::new();
    for (sequence, event) in events.iter().enumerate() {
        assert_eq!(event["sequence_number"], sequence);
        match event["type"].as_str().unwrap() {
            "response.output_item.added" => {
                assert!(open.insert(event["item"]["id"].as_str().unwrap().to_owned()));
            }
            "response.output_item.done" => {
                assert!(open.remove(event["item"]["id"].as_str().unwrap()));
                completed.push(event["item"].clone());
            }
            kind if kind.ends_with(".delta") => {
                assert!(open.contains(event["item_id"].as_str().unwrap()), "{event}");
            }
            _ => {}
        }
    }
    assert!(open.is_empty());
    assert_eq!(completed.len(), 3);
    assert_eq!(completed[0]["type"], "reasoning");
    assert_eq!(completed[0]["content"][0]["text"], "Run the test.");
    assert_eq!(
        completed[1]["arguments"],
        "{\"cmd\":\"python3 test_calc.py\"}"
    );
    assert_eq!(completed[2]["input"], "*** Begin Patch\n*** End Patch");
    let terminal = events.last().unwrap();
    assert_eq!(terminal["type"], "response.completed");
    assert_eq!(terminal["response"]["output"], Value::Array(completed));
    assert_eq!(terminal["response"]["usage"]["input_tokens"], 12);
    assert_eq!(terminal["response"]["usage"]["output_tokens"], 7);
    assert_eq!(
        terminal["response"]["usage"]["output_tokens_details"]["reasoning_tokens"],
        3
    );
}
