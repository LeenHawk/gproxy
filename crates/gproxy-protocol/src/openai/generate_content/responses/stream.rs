use serde::{Deserialize, Serialize, de};
use serde_json::Value;

use crate::openai::common::*;

use super::{
    ResponseContentPart, ResponseObject, ResponseOutputItem, ResponseReasoningSummaryPart,
};

#[derive(Debug, Clone, PartialEq)]
pub enum ResponseStreamEvent {
    Known(Box<KnownResponseStreamEvent>),
    Unknown(Value),
}

impl ResponseStreamEvent {
    pub fn event_name(&self) -> Option<&str> {
        match self {
            Self::Known(event) => Some(event.type_.as_str()),
            Self::Unknown(value) => value.get("type").and_then(Value::as_str),
        }
    }
}

impl Serialize for ResponseStreamEvent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Known(event) => event.serialize(serializer),
            Self::Unknown(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ResponseStreamEvent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let Some(type_name) = value.get("type").and_then(Value::as_str) else {
            return Ok(Self::Unknown(value));
        };
        let event_type =
            serde_json::from_value::<ResponseStreamEventType>(Value::String(type_name.to_owned()))
                .map_err(de::Error::custom)?;
        match event_type {
            ResponseStreamEventType::Known(_) => serde_json::from_value(value)
                .map(Box::new)
                .map(Self::Known)
                .map_err(de::Error::custom),
            ResponseStreamEventType::Unknown(_) => Ok(Self::Unknown(value)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnownResponseStreamEvent {
    #[serde(rename = "type")]
    pub type_: ResponseStreamEventTypeKnown,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<Box<ResponseObject>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<Box<ResponseOutputItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part: Option<ResponseContentPart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<Vec<StreamTokenLogprob>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotation: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotation_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_image_b64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_image_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_part: Option<ResponseReasoningSummaryPart>,
    #[serde(default, flatten)]
    pub rest: Rest,
}
