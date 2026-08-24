use serde_json::{Value, json};

use super::round_trip;
use crate::openai::generate_content::responses::ResponseStreamEvent;

#[test]
fn response_stream_event_payloads_round_trip_sparse_and_open_fields() {
    for event in [
        json!({
            "type":"response.output_text.delta","delta":"hi","item_id":"msg_1",
            "output_index":0,"future_delta":true
        }),
        json!({
            "type":"response.function_call_arguments.done","arguments":"{}",
            "output_index":0,"future_function":1
        }),
        json!({
            "type":"response.reasoning_summary_part.done","item_id":"rs_1",
            "output_index":0,"part":{"type":"summary_text","text":"summary"},
            "summary_index":0,"status":"incomplete","future_reasoning":true
        }),
        json!({
            "type":"error","code":null,"message":"failed","param":null,
            "future_error":{"retry":false}
        }),
        json!({
            "type":"response.output_text.annotation.added",
            "annotation":{"type":"future_annotation","nested":[{"x":1}]},
            "annotation_index":0,"content_index":0,"item_id":"msg_1","output_index":0
        }),
    ] {
        let parsed = round_trip::<ResponseStreamEvent>(event);
        assert!(matches!(parsed, ResponseStreamEvent::Known(_)));
    }

    let unknown = json!({
        "type":"response.future_event","sequence_number":7,"raw":{"nested":[{"x":1}]}
    });
    let parsed = round_trip::<ResponseStreamEvent>(unknown.clone());
    let ResponseStreamEvent::Unknown(raw) = parsed else {
        panic!("future event must remain a typed unknown object");
    };
    assert_eq!(serde_json::to_value(raw).unwrap(), unknown);

    let no_type = round_trip::<ResponseStreamEvent>(json!({"future_event":true}));
    assert!(matches!(no_type, ResponseStreamEvent::Unknown(_)));
    assert!(serde_json::from_value::<ResponseStreamEvent>(Value::Bool(true)).is_err());
}
