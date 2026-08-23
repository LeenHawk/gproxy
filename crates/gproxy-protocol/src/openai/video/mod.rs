mod request;
mod response;
mod types;

pub use request::*;
pub use response::*;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn video_models_round_trip_unknown_fields_and_variants() {
        let request = json!({
            "prompt":"continue",
            "seconds":"24",
            "video":{"id":"video_1", "future_ref":true},
            "future_request":1
        });
        let parsed: ExtendVideoRequest = serde_json::from_value(request.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), request);

        let response = json!({
            "id":"video_1",
            "created_at":1,
            "model":"sora-future",
            "object":"video",
            "progress":42,
            "seconds":"24",
            "size":"2048x2048",
            "status":"paused",
            "future_response":{"x":1}
        });
        let parsed: Video = serde_json::from_value(response.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), response);
    }
}
