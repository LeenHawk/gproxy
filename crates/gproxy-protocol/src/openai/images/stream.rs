use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::Rest;

use super::ImageUsage;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ImageStreamEvent {
    Partial(ImagePartialEvent),
    Completed(ImageCompletedEvent),
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImagePartialEvent {
    #[serde(rename = "type")]
    pub type_: ImagePartialEventType,
    pub b64_json: String,
    pub partial_image_index: u32,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImagePartialEventType {
    #[serde(rename = "image_generation.partial_image")]
    Generation,
    #[serde(rename = "image_edit.partial_image")]
    Edit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageCompletedEvent {
    #[serde(rename = "type")]
    pub type_: ImageCompletedEventType,
    pub b64_json: String,
    pub usage: ImageUsage,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageCompletedEventType {
    #[serde(rename = "image_generation.completed")]
    Generation,
    #[serde(rename = "image_edit.completed")]
    Edit,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::images::{CreateImageRequest, ImagesResponse};
    use serde_json::json;

    #[test]
    fn image_models_round_trip_unknown_fields_and_events() {
        let request = json!({
            "prompt": "draw",
            "quality": "future",
            "future_request": {"x": 1}
        });
        let parsed: CreateImageRequest = serde_json::from_value(request.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), request);

        let response = json!({
            "created": 1,
            "data": [{"b64_json": "abc", "future_image": true}],
            "future_response": 2
        });
        let parsed: ImagesResponse = serde_json::from_value(response.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), response);

        let event = json!({"type":"image_generation.future","payload":{"x":1}});
        let parsed: ImageStreamEvent = serde_json::from_value(event.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), event);
    }
}
