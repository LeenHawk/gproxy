use serde_json::json;

use super::round_trip;
use crate::openai::audio::TranscriptionStreamEvent;
use crate::openai::generate_content::chat::ChatCompletionMessageParam;
use crate::openai::generate_content::responses::{
    ResponseCaller, ResponseMessageItem, ResponseOutputMessageItem,
};

#[test]
fn retained_openai_unions_preserve_null_required_and_unknown_object_shapes() {
    let known = round_trip::<TranscriptionStreamEvent>(json!({
        "type":"transcript.text.delta",
        "delta":"hello",
        "future_delta":true
    }));
    assert!(matches!(known, TranscriptionStreamEvent::Delta(_)));

    for unknown in [
        json!({"type":"transcript.future","payload":{"x":1}}),
        json!({"type":"transcript.text.delta","future_known":true}),
    ] {
        let parsed = round_trip::<TranscriptionStreamEvent>(unknown);
        assert!(matches!(parsed, TranscriptionStreamEvent::Unknown(_)));
    }
    assert!(serde_json::from_value::<TranscriptionStreamEvent>(json!("not an event")).is_err());

    let function = round_trip::<ChatCompletionMessageParam>(json!({
        "role":"function",
        "name":"lookup",
        "content":null,
        "future_message":1
    }));
    assert!(matches!(
        function,
        ChatCompletionMessageParam::Function(message) if message.content.is_none()
    ));

    let message = round_trip::<ResponseMessageItem>(json!({
        "type":"message",
        "id":"msg_1",
        "role":"assistant",
        "content":[],
        "status":"completed",
        "future_output":true
    }));
    assert!(matches!(
        message,
        ResponseMessageItem::Output(message) if message.id == "msg_1"
    ));
    assert!(
        serde_json::from_value::<ResponseOutputMessageItem>(json!({
            "type":"message",
            "role":"assistant",
            "content":[],
            "status":"completed"
        }))
        .is_err()
    );

    let direct = round_trip::<ResponseCaller>(json!({"type":"direct","future":1}));
    assert!(matches!(direct, ResponseCaller::Direct(_)));
    let program = round_trip::<ResponseCaller>(json!({
        "type":"program",
        "caller_id":"program_1",
        "future":2
    }));
    assert!(matches!(program, ResponseCaller::Program(_)));
}
