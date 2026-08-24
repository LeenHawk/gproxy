use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    ResponseInputFile, ResponseInputImage, ResponseInputText, ResponseOutputText,
    ResponseReasoningText, ResponseRefusal,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseOutput {
    Text(String),
    Parts(Vec<ResponseToolOutputContentPart>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseToolOutputContentPart {
    #[serde(rename = "input_text")]
    InputText(ResponseInputText),
    #[serde(rename = "input_image")]
    InputImage(ResponseInputImage),
    #[serde(rename = "input_file")]
    InputFile(ResponseInputFile),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseMessageOutputContentPart {
    OutputText(ResponseOutputText),
    Refusal(ResponseRefusal),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseOutputContentPart {
    OutputText(ResponseOutputText),
    Refusal(ResponseRefusal),
    ReasoningText(ResponseReasoningText),
    Unknown(Value),
}

pub type ResponseContentPart = ResponseOutputContentPart;
