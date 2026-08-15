//! OpenAI Realtime API wire models (GA: WebSocket `GET /v1/realtime` and
//! WebRTC `POST /v1/realtime/calls`).
//!
//! These modules mirror Realtime JSON wire shapes only; transport (WebSocket
//! framing, SDP exchange) and provider conversion live outside this layer.

mod client_events;
mod items;
mod response;
mod server_events;
mod session;

pub use client_events::*;
pub use items::*;
pub use response::*;
pub use server_events::*;
pub use session::*;

use super::common::OpenAiWireModel;

pub type RealtimeWireModel = OpenAiWireModel<RealtimeClientEvent, RealtimeServerEvent>;

macro_rules! realtime_string_enum {
    ($outer:ident, $known:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
        #[serde(untagged)]
        #[non_exhaustive]
        pub enum $outer {
            Known($known),
            Unknown(String),
        }

        impl<'de> serde::Deserialize<'de> for $outer {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                crate::protocol::extensible::deserialize_extensible(d, Self::Known, Self::Unknown)
            }
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #[non_exhaustive]
        pub enum $known {
            $(#[serde(rename = $wire)] $variant,)+
        }
    };
}

pub(crate) use realtime_string_enum;

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn session_update_round_trips_ga_audio_shape() {
        let value = json!({
            "type": "session.update",
            "session": {
                "type": "realtime",
                "model": "gpt-realtime",
                "output_modalities": ["audio"],
                "instructions": "be brief",
                "audio": {
                    "input": {
                        "format": {"type": "audio/pcm", "rate": 24000},
                        "turn_detection": {"type": "semantic_vad", "eagerness": "low"}
                    },
                    "output": {"format": {"type": "audio/pcmu"}, "voice": "marin", "speed": 1.1}
                }
            }
        });
        let parsed: RealtimeClientEvent = serde_json::from_value(value.clone()).unwrap();
        let RealtimeClientEvent::SessionUpdate { session, .. } = &parsed else {
            panic!("expected session.update")
        };
        assert!(matches!(
            session
                .audio
                .as_ref()
                .unwrap()
                .input
                .as_ref()
                .unwrap()
                .format,
            Some(RealtimeAudioFormat::Pcm {
                rate: Some(24000),
                ..
            })
        ));
        assert_eq!(serde_json::to_value(&parsed).unwrap(), value);
    }

    #[test]
    fn server_events_parse_known_and_fall_back_to_unknown() {
        let known: RealtimeServerEvent = serde_json::from_value(json!({
            "type": "response.output_audio.delta",
            "response_id": "resp_1",
            "item_id": "item_1",
            "output_index": 0,
            "content_index": 0,
            "delta": "b64"
        }))
        .unwrap();
        let RealtimeServerEvent::Known(known) = known else {
            panic!("expected known event")
        };
        assert!(matches!(
            *known,
            KnownRealtimeServerEvent::OutputAudioDelta { .. }
        ));

        let unknown: RealtimeServerEvent = serde_json::from_value(json!({
            "type": "response.hologram.delta",
            "delta": "?"
        }))
        .unwrap();
        assert!(matches!(unknown, RealtimeServerEvent::Unknown(_)));
    }
}
