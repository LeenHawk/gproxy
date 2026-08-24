use serde_json::{Value, json};

use super::round_trip;
use crate::openai::generate_content::responses::{
    PromptVariableValue, ResponseInputContentPart, ResponseOutput, ResponseToolOutputContentPart,
};

#[test]
fn specialized_input_content_unions_round_trip_and_reject_audio() {
    round_trip::<PromptVariableValue>(json!("prompt text"));
    for part in [
        json!({"type":"input_text","text":"hello","future_text":true}),
        json!({"type":"input_image","image_url":"https://example.test/a.png","future_image":1}),
        json!({"type":"input_file","file_id":"file_1","future_file":{"x":1}}),
    ] {
        round_trip::<PromptVariableValue>(part);
    }

    round_trip::<ResponseOutput>(json!([
        {"type":"input_text","text":"done","future_text":true},
        {"type":"input_image","file_id":"file_image","future_image":1},
        {"type":"input_file","file_url":"https://example.test/a.txt","future_file":1}
    ]));

    let audio = json!({
        "type":"input_audio",
        "input_audio":{"data":"AA==","format":"wav"}
    });
    assert!(serde_json::from_value::<PromptVariableValue>(audio.clone()).is_err());
    assert!(serde_json::from_value::<ResponseOutput>(Value::Array(vec![audio.clone()])).is_err());
    assert!(serde_json::from_value::<ResponseToolOutputContentPart>(audio.clone()).is_err());
    round_trip::<ResponseInputContentPart>(audio);
}
