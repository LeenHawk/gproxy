mod events;
mod payloads;

use serde::{Deserialize, Serialize, de};
use serde_json::Value;

use crate::openai::common::{ResponseStreamEventType, ResponseStreamEventTypeKnown, Rest};

pub use events::*;
pub use payloads::*;

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseStreamEvent {
    Known(Box<KnownResponseStreamEvent>),
    Unknown(UnknownResponseStreamEvent),
}

impl ResponseStreamEvent {
    pub fn event_name(&self) -> Option<&str> {
        match self {
            Self::Known(event) => Some(event.event_name()),
            Self::Unknown(event) => event.type_.as_ref().map(ResponseStreamEventType::as_str),
        }
    }
}

impl Serialize for ResponseStreamEvent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Known(event) => event.serialize(serializer),
            Self::Unknown(event) => event.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ResponseStreamEvent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let Some(type_name) = value.get("type").and_then(Value::as_str) else {
            return serde_json::from_value(value)
                .map(Self::Unknown)
                .map_err(de::Error::custom);
        };
        let event_type =
            serde_json::from_value::<ResponseStreamEventType>(Value::String(type_name.to_owned()))
                .map_err(de::Error::custom)?;
        match event_type {
            ResponseStreamEventType::Known(_) => serde_json::from_value(value)
                .map(Box::new)
                .map(Self::Known)
                .map_err(de::Error::custom),
            ResponseStreamEventType::Unknown(_) => serde_json::from_value(value)
                .map(Self::Unknown)
                .map_err(de::Error::custom),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnknownResponseStreamEvent {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<ResponseStreamEventType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

impl KnownResponseStreamEvent {
    pub fn event_type(&self) -> ResponseStreamEventTypeKnown {
        use ResponseStreamEventTypeKnown as T;
        match self {
            Self::ResponseCreated(_) => T::ResponseCreated,
            Self::ResponseInProgress(_) => T::ResponseInProgress,
            Self::ResponseCompleted(_) => T::ResponseCompleted,
            Self::ResponseFailed(_) => T::ResponseFailed,
            Self::ResponseIncomplete(_) => T::ResponseIncomplete,
            Self::ResponseQueued(_) => T::ResponseQueued,
            Self::ResponseOutputItemAdded(_) => T::ResponseOutputItemAdded,
            Self::ResponseOutputItemDone(_) => T::ResponseOutputItemDone,
            Self::ResponseContentPartAdded(_) => T::ResponseContentPartAdded,
            Self::ResponseContentPartDone(_) => T::ResponseContentPartDone,
            Self::ResponseOutputTextDelta(_) => T::ResponseOutputTextDelta,
            Self::ResponseOutputTextDone(_) => T::ResponseOutputTextDone,
            Self::ResponseOutputTextAnnotationAdded(_) => T::ResponseOutputTextAnnotationAdded,
            Self::ResponseFunctionCallArgumentsDelta(_) => T::ResponseFunctionCallArgumentsDelta,
            Self::ResponseFunctionCallArgumentsDone(_) => T::ResponseFunctionCallArgumentsDone,
            Self::ResponseCustomToolCallInputDelta(_) => T::ResponseCustomToolCallInputDelta,
            Self::ResponseCustomToolCallInputDone(_) => T::ResponseCustomToolCallInputDone,
            Self::ResponseRefusalDelta(_) => T::ResponseRefusalDelta,
            Self::ResponseRefusalDone(_) => T::ResponseRefusalDone,
            Self::ResponseReasoningSummaryPartAdded(_) => T::ResponseReasoningSummaryPartAdded,
            Self::ResponseReasoningSummaryPartDone(_) => T::ResponseReasoningSummaryPartDone,
            Self::ResponseReasoningSummaryTextDelta(_) => T::ResponseReasoningSummaryTextDelta,
            Self::ResponseReasoningSummaryTextDone(_) => T::ResponseReasoningSummaryTextDone,
            Self::ResponseReasoningTextDelta(_) => T::ResponseReasoningTextDelta,
            Self::ResponseReasoningTextDone(_) => T::ResponseReasoningTextDone,
            Self::ResponseAudioDelta(_) => T::ResponseAudioDelta,
            Self::ResponseAudioDone(_) => T::ResponseAudioDone,
            Self::ResponseAudioTranscriptDelta(_) => T::ResponseAudioTranscriptDelta,
            Self::ResponseAudioTranscriptDone(_) => T::ResponseAudioTranscriptDone,
            Self::ResponseImageGenerationCallCompleted(_) => {
                T::ResponseImageGenerationCallCompleted
            }
            Self::ResponseImageGenerationCallGenerating(_) => {
                T::ResponseImageGenerationCallGenerating
            }
            Self::ResponseImageGenerationCallInProgress(_) => {
                T::ResponseImageGenerationCallInProgress
            }
            Self::ResponseImageGenerationCallPartialImage(_) => {
                T::ResponseImageGenerationCallPartialImage
            }
            Self::ResponseFileSearchCallInProgress(_) => T::ResponseFileSearchCallInProgress,
            Self::ResponseFileSearchCallSearching(_) => T::ResponseFileSearchCallSearching,
            Self::ResponseFileSearchCallCompleted(_) => T::ResponseFileSearchCallCompleted,
            Self::ResponseWebSearchCallInProgress(_) => T::ResponseWebSearchCallInProgress,
            Self::ResponseWebSearchCallSearching(_) => T::ResponseWebSearchCallSearching,
            Self::ResponseWebSearchCallCompleted(_) => T::ResponseWebSearchCallCompleted,
            Self::ResponseCodeInterpreterCallInProgress(_) => {
                T::ResponseCodeInterpreterCallInProgress
            }
            Self::ResponseCodeInterpreterCallInterpreting(_) => {
                T::ResponseCodeInterpreterCallInterpreting
            }
            Self::ResponseCodeInterpreterCallCompleted(_) => {
                T::ResponseCodeInterpreterCallCompleted
            }
            Self::ResponseCodeInterpreterCallCodeDelta(_) => {
                T::ResponseCodeInterpreterCallCodeDelta
            }
            Self::ResponseCodeInterpreterCallCodeDone(_) => T::ResponseCodeInterpreterCallCodeDone,
            Self::ResponseMcpCallArgumentsDelta(_) => T::ResponseMcpCallArgumentsDelta,
            Self::ResponseMcpCallArgumentsDone(_) => T::ResponseMcpCallArgumentsDone,
            Self::ResponseMcpCallInProgress(_) => T::ResponseMcpCallInProgress,
            Self::ResponseMcpCallCompleted(_) => T::ResponseMcpCallCompleted,
            Self::ResponseMcpCallFailed(_) => T::ResponseMcpCallFailed,
            Self::ResponseMcpListToolsInProgress(_) => T::ResponseMcpListToolsInProgress,
            Self::ResponseMcpListToolsCompleted(_) => T::ResponseMcpListToolsCompleted,
            Self::ResponseMcpListToolsFailed(_) => T::ResponseMcpListToolsFailed,
            Self::Error(_) => T::Error,
        }
    }

    pub fn event_name(&self) -> &'static str {
        self.event_type().as_str()
    }
}
