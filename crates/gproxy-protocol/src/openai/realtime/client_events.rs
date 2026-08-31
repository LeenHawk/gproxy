use serde::{Deserialize, Serialize, de};
use serde_json::Value;

use crate::openai::common::Rest;

use super::{RealtimeItem, RealtimeResponseOptions, RealtimeSession};

/// A client event, or whatever the client actually sent.
///
/// The server side has always had this fallback; the client side did not, so an
/// event OpenAI adds — they have already added a whole translation surface with
/// `session.close` and `session.input_audio_buffer.append` — would have failed the
/// connection outright instead of reaching an upstream that understands it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeClientEvent {
    Known(Box<KnownRealtimeClientEvent>),
    Unknown(UnknownRealtimeClientEvent),
}

impl<'de> Deserialize<'de> for RealtimeClientEvent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        if let Ok(event) = serde_json::from_value::<KnownRealtimeClientEvent>(value.clone()) {
            return Ok(Self::Known(Box::new(event)));
        }
        serde_json::from_value(value)
            .map(Self::Unknown)
            .map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnknownRealtimeClientEvent {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum KnownRealtimeClientEvent {
    #[serde(rename = "session.update")]
    SessionUpdate {
        session: RealtimeSession,
        #[serde(skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "input_audio_buffer.append")]
    InputAudioBufferAppend {
        audio: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "input_audio_buffer.commit")]
    InputAudioBufferCommit {
        #[serde(skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "input_audio_buffer.clear")]
    InputAudioBufferClear {
        #[serde(skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "output_audio_buffer.clear")]
    OutputAudioBufferClear {
        #[serde(skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "conversation.item.create")]
    ConversationItemCreate {
        item: RealtimeItem,
        #[serde(skip_serializing_if = "Option::is_none")]
        previous_item_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "conversation.item.retrieve")]
    ConversationItemRetrieve {
        item_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "conversation.item.truncate")]
    ConversationItemTruncate {
        item_id: String,
        content_index: u32,
        audio_end_ms: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "conversation.item.delete")]
    ConversationItemDelete {
        item_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "response.create")]
    ResponseCreate {
        #[serde(skip_serializing_if = "Option::is_none")]
        response: Option<RealtimeResponseOptions>,
        #[serde(skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "response.cancel")]
    ResponseCancel {
        #[serde(skip_serializing_if = "Option::is_none")]
        response_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
}
