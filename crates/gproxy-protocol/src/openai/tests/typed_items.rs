use serde_json::json;

use super::round_trip;
use crate::openai::generate_content::responses::TypedResponseItem;

#[test]
fn typed_response_items_preserve_required_nullable_fields() {
    let image = round_trip::<TypedResponseItem>(json!({
        "type":"image_generation_call",
        "id":"image_1",
        "result":null,
        "status":"in_progress",
        "future_image":true
    }));
    assert!(matches!(
        image,
        TypedResponseItem::ImageGenerationCall { result: None, .. }
    ));

    let code = round_trip::<TypedResponseItem>(json!({
        "type":"code_interpreter_call",
        "id":"code_1",
        "code":null,
        "container_id":"container_1",
        "outputs":null,
        "status":"in_progress",
        "future_code":true
    }));
    assert!(matches!(
        code,
        TypedResponseItem::CodeInterpreterCall {
            code: None,
            outputs: None,
            ..
        }
    ));
}
