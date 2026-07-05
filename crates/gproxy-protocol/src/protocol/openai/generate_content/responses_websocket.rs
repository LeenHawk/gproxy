use serde::{Deserialize, Serialize};

use super::super::common::*;
use super::{ResponseCreateRequest, ResponseStreamEvent};

pub type ResponseWebSocketWireModel =
    OpenAiWireModel<ResponseWebSocketRequest, ResponseStreamEvent>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
pub enum ResponseWebSocketRequest {
    #[serde(rename = "response.create")]
    ResponseCreate(ResponseCreateWebSocketRequest),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ResponseCreateWebSocketRequest {
    #[serde(flatten)]
    pub response: ResponseCreateRequest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generate: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_metadata: Option<Metadata>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::openai::{OpenAiModelId, ResponseInput};

    #[test]
    fn response_create_frame_round_trips_websocket_fields() {
        let value = json!({
            "type": "response.create",
            "model": "gpt-x",
            "input": "hello",
            "stream": true,
            "generate": false,
            "client_metadata": {
                "x-codex-installation-id": "installation-1"
            }
        });

        let parsed: ResponseWebSocketRequest = serde_json::from_value(value).unwrap();
        let ResponseWebSocketRequest::ResponseCreate(frame) = parsed;
        assert_eq!(
            frame.response.model,
            Some(OpenAiModelId::Unknown("gpt-x".to_owned()))
        );
        assert_eq!(
            frame.response.input,
            Some(ResponseInput::Text("hello".to_owned()))
        );
        assert_eq!(frame.response.stream, Some(true));
        assert_eq!(frame.generate, Some(false));
        assert_eq!(
            frame
                .client_metadata
                .as_ref()
                .and_then(|m| m.get("x-codex-installation-id")),
            Some(&"installation-1".to_owned())
        );

        let serialized = serde_json::to_value(ResponseWebSocketRequest::ResponseCreate(frame))
            .expect("serialize websocket response.create");
        assert_eq!(serialized["type"], json!("response.create"));
        assert_eq!(serialized["generate"], json!(false));
    }
}
