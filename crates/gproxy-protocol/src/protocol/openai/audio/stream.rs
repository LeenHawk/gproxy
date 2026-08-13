use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de};
use serde_json::Value;

use super::super::common::Extra;
use super::{AudioTokenUsage, TranscriptionLanguage, TranscriptionLogprob};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum TranscriptionStreamEvent {
    Known(KnownTranscriptionStreamEvent),
    Unknown(UnknownTranscriptionStreamEvent),
}

impl<'de> Deserialize<'de> for TranscriptionStreamEvent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        match value.get("type").and_then(Value::as_str) {
            Some("transcript.text.delta" | "transcript.text.done" | "transcript.text.segment") => {
                if let Ok(event) = serde_json::from_value(value.clone()) {
                    return Ok(Self::Known(event));
                }
                serde_json::from_value(value)
                    .map(Self::Unknown)
                    .map_err(de::Error::custom)
            }
            _ => serde_json::from_value(value)
                .map(Self::Unknown)
                .map_err(de::Error::custom),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum KnownTranscriptionStreamEvent {
    #[serde(rename = "transcript.text.delta")]
    TextDelta {
        delta: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<TranscriptionLogprob>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        segment_id: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "transcript.text.done")]
    TextDone {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        languages: Option<Vec<TranscriptionLanguage>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<TranscriptionLogprob>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<AudioTokenUsage>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "transcript.text.segment")]
    TextSegment {
        id: String,
        end: f64,
        speaker: String,
        start: f64,
        text: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct UnknownTranscriptionStreamEvent {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}
