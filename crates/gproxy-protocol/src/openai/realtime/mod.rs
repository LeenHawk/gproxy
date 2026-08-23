//! OpenAI Realtime call handshake wire.
//!
//! The local OpenAI snapshot has no Realtime reference page. These call and
//! session shapes follow the v2 protocol model and captured request evidence.

mod session;

pub use session::*;

use serde::{Deserialize, Serialize};

use crate::openai::common::{OpenAiModelId, Rest};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateRealtimeCallRequest {
    pub sdp: String,
    pub session: RealtimeSession,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<OpenAiModelId>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

pub type CreateRealtimeCallResponse = String;

#[cfg(test)]
mod tests {
    #[test]
    fn realtime_call_round_trip_preserves_session_extensions() {
        let value = serde_json::json!({
            "sdp":"v=0\r\n",
            "model":"public-route",
            "session": {
                "type":"realtime",
                "model":"gpt-realtime",
                "audio":{"input":{"future_audio":true}},
                "future_session":1
            },
            "future_call":true
        });
        let parsed: super::CreateRealtimeCallRequest =
            serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    }
}
