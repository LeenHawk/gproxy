use serde::{Deserialize, Serialize, de};
use serde_json::Value;

use crate::openai::common::Rest;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeItem {
    Known(KnownRealtimeItem),
    Unknown(UnknownRealtimeItem),
}

impl<'de> Deserialize<'de> for RealtimeItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        if let Ok(item) = serde_json::from_value(value.clone()) {
            return Ok(Self::Known(item));
        }
        serde_json::from_value(value)
            .map(Self::Unknown)
            .map_err(de::Error::custom)
    }
}

extensible_string!(RealtimeRole, KnownRealtimeRole {
    User => "user", Assistant => "assistant", System => "system",
});
extensible_string!(RealtimeItemStatus, KnownRealtimeItemStatus {
    InProgress => "in_progress", Completed => "completed", Incomplete => "incomplete",
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum KnownRealtimeItem {
    #[serde(rename = "message")]
    Message {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        role: RealtimeRole,
        content: Vec<RealtimeContentPart>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<RealtimeItemStatus>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "function_call")]
    FunctionCall {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        call_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        arguments: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<RealtimeItemStatus>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "function_call_output")]
    FunctionCallOutput {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        call_id: String,
        output: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<RealtimeItemStatus>,
        #[serde(default, flatten)]
        rest: Rest,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct UnknownRealtimeItem {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeContentPart {
    #[serde(rename = "input_text")]
    InputText {
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "input_audio")]
    InputAudio {
        #[serde(skip_serializing_if = "Option::is_none")]
        audio: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        transcript: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "text")]
    Text {
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "audio")]
    Audio {
        #[serde(skip_serializing_if = "Option::is_none")]
        audio: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        transcript: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "output_text")]
    OutputText {
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "output_audio")]
    OutputAudio {
        #[serde(skip_serializing_if = "Option::is_none")]
        audio: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        transcript: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
}
