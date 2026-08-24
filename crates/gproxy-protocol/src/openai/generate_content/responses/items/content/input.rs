use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    ResponseInputAudio, ResponseInputFile, ResponseInputImage, ResponseInputText,
    ResponseMessageOutputContentPart,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseEasyInputContent {
    Text(String),
    Parts(Vec<ResponseInputContentPart>),
    OutputParts(Vec<ResponseMessageOutputContentPart>),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseInputContentPart {
    #[serde(rename = "input_text")]
    InputText(ResponseInputText),
    #[serde(rename = "input_image")]
    InputImage(ResponseInputImage),
    #[serde(rename = "input_file")]
    InputFile(ResponseInputFile),
    #[serde(rename = "input_audio")]
    InputAudio(ResponseInputAudio),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum PromptVariableInputContentPart {
    #[serde(rename = "input_text")]
    InputText(ResponseInputText),
    #[serde(rename = "input_image")]
    InputImage(ResponseInputImage),
    #[serde(rename = "input_file")]
    InputFile(ResponseInputFile),
}
