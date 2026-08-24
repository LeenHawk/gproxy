use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::openai::common::ImageStreamEventType;
use crate::openai::images::{CreateImageRequest, ImagesResponse};

use super::*;

fn round_trip<T>(value: &Value) -> T
where
    T: DeserializeOwned + serde::Serialize,
{
    let parsed = serde_json::from_value::<T>(value.clone()).expect("decode image wire value");
    assert_eq!(
        serde_json::to_value(&parsed).expect("encode image wire value"),
        *value
    );
    parsed
}

#[test]
fn image_models_round_trip_unknown_fields_and_events() {
    let request = json!({
        "prompt": "draw",
        "quality": "future",
        "future_request": {"x": 1}
    });
    let parsed = round_trip::<CreateImageRequest>(&request);
    assert_eq!(parsed.rest["future_request"]["x"], 1);

    let response = json!({
        "created": 1,
        "data": [{"b64_json": "abc", "future_image": true}],
        "future_response": 2
    });
    round_trip::<ImagesResponse>(&response);

    let generation_partial = json!({
        "type":"image_generation.partial_image",
        "b64_json":"partial",
        "partial_image_index":0,
        "future_partial":true
    });
    assert!(matches!(
        round_trip::<ImageGenerationStreamEvent>(&generation_partial),
        ImageGenerationStreamEvent::Known(KnownImageGenerationStreamEvent::PartialImage(_))
    ));
    assert!(matches!(
        round_trip::<ImageStreamEvent>(&generation_partial),
        ImageStreamEvent::Known(KnownImageStreamEvent::ImageGenerationPartialImage(_))
    ));

    let generation_completed = json!({
        "type":"image_generation.completed",
        "b64_json":"final",
        "future_completed":true
    });
    let parsed = round_trip::<ImageGenerationStreamEvent>(&generation_completed);
    assert!(matches!(
        parsed,
        ImageGenerationStreamEvent::Known(KnownImageGenerationStreamEvent::Completed(
            ImageCompletedEvent { usage: None, .. }
        ))
    ));
    assert!(matches!(
        round_trip::<ImageStreamEvent>(&generation_completed),
        ImageStreamEvent::Known(KnownImageStreamEvent::ImageGenerationCompleted(_))
    ));

    let edit_partial = json!({
        "type":"image_edit.partial_image",
        "b64_json":"partial",
        "partial_image_index":1,
        "future_partial":true
    });
    assert!(matches!(
        round_trip::<ImageEditStreamEvent>(&edit_partial),
        ImageEditStreamEvent::Known(KnownImageEditStreamEvent::PartialImage(_))
    ));
    assert!(matches!(
        round_trip::<ImageStreamEvent>(&edit_partial),
        ImageStreamEvent::Known(KnownImageStreamEvent::ImageEditPartialImage(_))
    ));

    let edit_completed = json!({
        "type":"image_edit.completed",
        "b64_json":"final",
        "usage":{
            "input_tokens":1,
            "input_tokens_details":{"image_tokens":0,"text_tokens":1},
            "output_tokens":2,
            "total_tokens":3
        },
        "future_completed":true
    });
    assert!(matches!(
        round_trip::<ImageEditStreamEvent>(&edit_completed),
        ImageEditStreamEvent::Known(KnownImageEditStreamEvent::Completed(_))
    ));
    assert!(matches!(
        round_trip::<ImageStreamEvent>(&edit_completed),
        ImageStreamEvent::Known(KnownImageStreamEvent::ImageEditCompleted(_))
    ));

    let future = json!({"type":"image_generation.future","payload":{"x":1}});
    let parsed = round_trip::<ImageGenerationStreamEvent>(&future);
    let ImageGenerationStreamEvent::Unknown(event) = parsed else {
        panic!("future generation event must remain typed unknown");
    };
    assert!(matches!(
        event.type_,
        Some(ImageStreamEventType::Unknown(value)) if value == "image_generation.future"
    ));
    assert_eq!(event.rest["payload"]["x"], 1);

    let missing_type = json!({"future_event":true});
    assert!(matches!(
        round_trip::<ImageEditStreamEvent>(&missing_type),
        ImageEditStreamEvent::Unknown(UnknownImageStreamEvent { type_: None, .. })
    ));
    assert!(serde_json::from_value::<ImageGenerationStreamEvent>(edit_partial).is_err());
    assert!(serde_json::from_value::<ImageEditStreamEvent>(generation_partial).is_err());
    assert!(serde_json::from_value::<ImageGenerationStreamEvent>(json!("invalid")).is_err());
}
