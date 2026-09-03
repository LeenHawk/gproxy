use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::{Rest, StreamTokenLogprob};

use super::super::super::ResponseContentPart;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseContentPartEvent {
    pub content_index: u32,
    pub item_id: String,
    pub output_index: u32,
    pub part: ResponseContentPart,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseOutputTextDeltaEvent {
    // The Codex backend can omit the content index on sparse interrupted frames.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_index: Option<u32>,
    pub delta: String,
    pub item_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<Vec<StreamTokenLogprob>>,
    pub output_index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseOutputTextDoneEvent {
    pub content_index: u32,
    pub item_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<Vec<StreamTokenLogprob>>,
    pub output_index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    pub text: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseOutputTextAnnotationEvent {
    // OpenAI deliberately documents the streamed annotation payload as unknown.
    pub annotation: Value,
    pub annotation_index: u32,
    pub content_index: u32,
    pub item_id: String,
    pub output_index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseContentDeltaEvent {
    pub content_index: u32,
    pub delta: String,
    pub item_id: String,
    pub output_index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseRefusalDoneEvent {
    pub content_index: u32,
    pub item_id: String,
    pub output_index: u32,
    pub refusal: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseContentTextDoneEvent {
    pub content_index: u32,
    pub item_id: String,
    pub output_index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    pub text: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseAudioDeltaEvent {
    pub delta: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}
