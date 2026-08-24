//! OpenAI Realtime handshake, session, and event wire models.
//!
//! The local OpenAI snapshot has no Realtime reference page. These call and
//! session shapes follow the v2 protocol model and captured request evidence.

macro_rules! extensible_string {
    ($name:ident, $known:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(untagged)]
        #[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
        pub enum $name {
            Known($known),
            Unknown(String),
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
        pub enum $known {
            $(#[serde(rename = $wire)] $variant),+
        }
    };
}

mod audio;
mod client_events;
mod items;
mod response;
mod server_events;
mod session;

pub use audio::*;
pub use client_events::*;
pub use items::*;
pub use response::*;
pub use server_events::*;
pub use session::*;

use serde::{Deserialize, Serialize};

use crate::openai::common::{OpenAiModelId, OpenAiWireModel, Rest};

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
pub type RealtimeWireModel = OpenAiWireModel<RealtimeClientEvent, RealtimeServerEvent>;

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn realtime_call_round_trip_preserves_session_extensions() {
        let value = json!({
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

    #[test]
    fn realtime_event_unions_round_trip_known_unknown_and_evolved_payloads() {
        let client = json!({
            "type":"session.update",
            "event_id":"evt_1",
            "session":{"type":"realtime","model":"gpt-realtime"},
            "future_client":true
        });
        let parsed: RealtimeClientEvent = serde_json::from_value(client.clone()).unwrap();
        assert!(matches!(&parsed, RealtimeClientEvent::SessionUpdate { .. }));
        assert_eq!(serde_json::to_value(parsed).unwrap(), client);

        let item = json!({
            "type":"message",
            "id":"item_1",
            "role":"assistant",
            "content":[{"type":"output_audio","audio":"AAEC","future_part":1}],
            "future_item":true
        });
        let parsed: RealtimeItem = serde_json::from_value(item.clone()).unwrap();
        assert!(matches!(
            &parsed,
            RealtimeItem::Known(KnownRealtimeItem::Message { .. })
        ));
        assert_eq!(serde_json::to_value(parsed).unwrap(), item);

        let server = json!({
            "type":"response.output_audio.delta",
            "response_id":"resp_1",
            "item_id":"item_1",
            "output_index":0,
            "content_index":0,
            "delta":"AAEC",
            "future_server":2
        });
        let parsed: RealtimeServerEvent = serde_json::from_value(server.clone()).unwrap();
        assert!(matches!(
            &parsed,
            RealtimeServerEvent::Known(event)
                if matches!(event.as_ref(), KnownRealtimeServerEvent::OutputAudioDelta(_))
        ));
        assert_eq!(serde_json::to_value(parsed).unwrap(), server);

        for unknown in [
            json!({"type":"response.hologram.delta","delta":"?","future":3}),
            json!({"type":"response.output_audio.delta","delta":"missing required fields"}),
        ] {
            let parsed: RealtimeServerEvent = serde_json::from_value(unknown.clone()).unwrap();
            assert!(matches!(&parsed, RealtimeServerEvent::Unknown(_)));
            assert_eq!(serde_json::to_value(parsed).unwrap(), unknown);
        }

        let evolved_item = json!({
            "type":"message",
            "role":"assistant",
            "content":[{"type":"hologram","payload":"?"}]
        });
        let parsed: RealtimeItem = serde_json::from_value(evolved_item.clone()).unwrap();
        assert!(matches!(&parsed, RealtimeItem::Unknown(_)));
        assert_eq!(serde_json::to_value(parsed).unwrap(), evolved_item);
    }
}
