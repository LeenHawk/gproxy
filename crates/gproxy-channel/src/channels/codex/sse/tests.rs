use super::*;
use crate::channels::codex::shape::{self, tools::NativeTool};
use serde_json::{Value, json};

fn encode(events: &[Value]) -> String {
    events
        .iter()
        .map(|event| {
            format!(
                "event: {}\r\ndata: {event}\r\n\r\n",
                event["type"].as_str().unwrap()
            )
        })
        .collect()
}
fn decode(chunks: Vec<Bytes>) -> Vec<Value> {
    let mut decoder = SseDecoder::new(SSE_LIMITS);
    chunks
        .iter()
        .flat_map(|chunk| decoder.push(chunk).unwrap())
        .filter_map(|frame| match frame {
            SseFrame::Event(event) => Some(serde_json::from_str(&event.data).unwrap()),
            _ => None,
        })
        .collect()
}
fn run(events: &[Value], aliases: Aliases) -> Vec<Value> {
    let mut codec = Codec::new(aliases);
    let mut output = Vec::new();
    for chunk in encode(events).as_bytes().chunks(7) {
        output.extend(codec.push(chunk).unwrap());
    }
    output.extend(codec.finish().unwrap());
    decode(output)
}
fn completed(output: Value) -> Value {
    json!({"type":"response.completed","response":{"id":"resp_1","created_at":1,"object":"response","status":"completed",
        "output":output,"usage":{"input_tokens":20,"input_tokens_details":{"cached_tokens":5},"output_tokens":2}}})
}

#[test]
fn sparse_stream_repairs_tool_lifecycle_before_exact_terminal() {
    let events = run(
        &[
            json!({"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"f1","call_id":"c1","name":"f","arguments":""}}),
            json!({"type":"response.function_call_arguments.delta","output_index":0,"item_id":"f1","delta":"{}"}),
            json!({"type":"response.output_item.added","output_index":1,"item":{"type":"custom_tool_call","id":"t1","call_id":"c2","name":"custom","input":""}}),
            json!({"type":"response.custom_tool_call_input.delta","output_index":1,"item_id":"t1","delta":"patch"}),
            json!({"type":"response.output_text.delta","output_index":2,"item_id":"m1","content_index":0,"delta":"完成"}),
            completed(json!([])),
        ],
        Aliases::default(),
    );
    assert_eq!(events[0]["type"], "response.created");
    let names: Vec<_> = events
        .iter()
        .map(|event| event["type"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"response.function_call_arguments.done"));
    assert!(names.contains(&"response.custom_tool_call_input.done"));
    assert_eq!(
        events.last().unwrap()["response"]["output"][2]["content"][0]["text"],
        "完成"
    );
    assert_eq!(
        events.last().unwrap()["response"]["usage"]["input_tokens"],
        20
    );
    for (index, event) in events.iter().enumerate() {
        assert_eq!(event["sequence_number"], index);
    }
    serde_json::from_value::<gproxy_protocol::wire::openai::responses::stream::StreamEvent>(
        events[0].clone(),
    )
    .unwrap();
}

#[test]
fn aliases_are_request_local_and_restore_replay_ids() {
    let (_, aliases) = shape::request(&Bytes::from(json!({"tools":[{"type":"shell"}],"input":[
        {"type":"shell_call","id":"shell_old","call_id":"call_old","action":{"commands":["pwd"]}}
    ]}).to_string())).unwrap();
    let mapped = aliases.ids.keys().next().unwrap().clone();
    let call = json!({"type":"function_call","id":mapped,"call_id":"call_new","name":"shell_command","arguments":"{\"command\":\"pwd\",\"workdir\":\"/repo\"}"});
    let input = vec![
        json!({"type":"response.output_item.done","output_index":0,"item":call}),
        completed(json!([])),
    ];
    let restored = run(&input, aliases);
    assert_eq!(restored[1]["item"]["type"], "shell_call");
    assert_eq!(restored[1]["item"]["id"], "shell_old");
    assert_eq!(
        restored.last().unwrap()["response"]["output"][0]["action"]["commands"],
        json!(["pwd"])
    );
    let untouched = run(&input, Aliases::default());
    assert_eq!(
        untouched.last().unwrap()["response"]["output"][0]["type"],
        "function_call"
    );
    assert_eq!(
        untouched.last().unwrap()["response"]["output"][0]["name"],
        "shell_command"
    );
}

#[test]
fn terminal_only_native_tools_are_restored_and_lifecycle_is_repaired() {
    for (kind, name, call) in [
        (
            NativeTool::ApplyPatch,
            "apply_patch",
            json!({"type":"custom_tool_call","id":"ct1","call_id":"c1","name":"apply_patch","input":"*** Begin Patch\n*** Add File: a.txt\n+hello\n*** End Patch\n"}),
        ),
        (
            NativeTool::LocalShell,
            "shell_command",
            json!({"type":"function_call","id":"fc1","call_id":"c1","name":"shell_command","arguments":"{\"command\":\"pwd\",\"workdir\":\"/repo\"}"}),
        ),
    ] {
        let mut aliases = Aliases::default();
        aliases.tools.insert(name.into(), kind);
        let events = run(&[completed(json!([call]))], aliases);
        assert_eq!(events.len(), 4);
        assert_eq!(events[1]["type"], "response.output_item.added");
        assert_eq!(events[2]["type"], "response.output_item.done");
        assert_eq!(events[1]["item"], events[3]["response"]["output"][0]);
    }
}

#[test]
fn sparse_tool_deltas_before_metadata_are_recovered_without_guessing_ids() {
    let events = run(
        &[
            json!({"type":"response.function_call_arguments.delta","output_index":0,"item_id":"fc1","delta":"{}"}),
            completed(
                json!([{"type":"function_call","id":"fc1","call_id":"real_call","name":"f","arguments":"{}"}]),
            ),
        ],
        Aliases::default(),
    );
    assert_eq!(events[1]["type"], "response.output_item.added");
    assert_eq!(events[2]["type"], "response.function_call_arguments.delta");
    assert_eq!(
        events.last().unwrap()["response"]["output"][0]["call_id"],
        "real_call"
    );
}

#[test]
fn unterminated_or_malformed_streams_never_synthesize_success() {
    for bytes in [
        "data: [DONE]\n\n",
        "data: {broken}\n\n",
        "data: {\"type\":\"response.in_progress\",\"response\":{\"id\":\"resp_1\"}}\n\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\"}}",
    ] {
        let mut codec = Codec::new(Aliases::default());
        let result = codec.push(bytes.as_bytes()).and_then(|_| codec.finish());
        assert!(result.is_err(), "{bytes}");
    }
    let events = run(
        &[json!({"type":"error","code":"rate_limit","message":"try later"})],
        Aliases::default(),
    );
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["type"], "error");
}

#[tokio::test]
async fn upstream_transport_error_is_forwarded_without_a_terminal_event() {
    let body = HttpBody::Stream(Box::pin(stream::iter(vec![Err(std::io::Error::other(
        "interrupted",
    )
    .into())])));
    let HttpBody::Stream(mut output) = normalize(body, Aliases::default()) else {
        panic!("stream")
    };
    assert_eq!(
        output.next().await.unwrap().unwrap_err().to_string(),
        "interrupted"
    );
    assert!(output.next().await.is_none());
}

#[test]
fn incomplete_tool_input_does_not_hide_the_upstream_failure() {
    let mut aliases = Aliases::default();
    aliases
        .tools
        .insert("apply_patch".into(), NativeTool::ApplyPatch);
    let failure = json!({"type":"response.failed","response":{"id":"resp_1","status":"failed","output":[],"error":{"code":"server_error","message":"upstream interrupted"}}});
    let events = run(
        &[
            json!({"type":"response.output_item.added","output_index":0,"item":{"type":"custom_tool_call","id":"ct1","call_id":"c1","name":"apply_patch","input":""}}),
            json!({"type":"response.custom_tool_call_input.delta","output_index":0,"item_id":"ct1","delta":"*** Begin Patch\n"}),
            failure.clone(),
        ],
        aliases,
    );
    let terminal = events.last().unwrap();
    assert_eq!(terminal["type"], "response.failed");
    assert_eq!(terminal["response"]["error"], failure["response"]["error"]);
    assert_eq!(terminal["response"]["output"][0]["status"], "incomplete");
    assert!(
        !events
            .iter()
            .any(|event| event["type"] == "response.output_item.done")
    );
}
