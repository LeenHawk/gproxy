use serde::{Deserialize, Serialize};

use crate::openai::common::Rest;

use super::{RealtimeItem, RealtimeResponseOptions, RealtimeSession};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeClientEvent {
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
