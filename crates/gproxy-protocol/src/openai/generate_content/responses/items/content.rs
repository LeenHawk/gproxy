use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::*;

use super::ResponseAnnotation;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseOutput {
    Text(String),
    Parts(Vec<ResponseToolOutputContentPart>),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseEasyInputContent {
    Text(String),
    Parts(Vec<ResponseInputContentPart>),
    OutputParts(Vec<ResponseMessageOutputContentPart>),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseInputContentPart {
    InputText(ResponseInputText),
    InputImage(ResponseInputImage),
    InputFile(ResponseInputFile),
    InputAudio(ResponseInputAudio),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseInputText {
    #[serde(rename = "type")]
    pub type_: ResponseInputTextType,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseInputTextType {
    #[serde(rename = "input_text")]
    InputText,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseInputImage {
    #[serde(rename = "type")]
    pub type_: ResponseInputImageType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<DetailLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseInputImageType {
    #[serde(rename = "input_image")]
    InputImage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseInputFile {
    #[serde(rename = "type")]
    pub type_: ResponseInputFileType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<InputFileDetailLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseInputFileType {
    #[serde(rename = "input_file")]
    InputFile,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseInputAudio {
    #[serde(rename = "type")]
    pub type_: ResponseInputAudioType,
    pub input_audio: InputAudioContent,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseInputAudioType {
    #[serde(rename = "input_audio")]
    InputAudio,
}

pub type ResponseToolOutputContentPart = ResponseInputContentPart;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseMessageOutputContentPart {
    OutputText(ResponseOutputText),
    Refusal(ResponseRefusal),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseOutputContentPart {
    OutputText(ResponseOutputText),
    Refusal(ResponseRefusal),
    ReasoningText(ResponseReasoningText),
    Unknown(Value),
}

pub type ResponseContentPart = ResponseOutputContentPart;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseOutputText {
    #[serde(rename = "type")]
    pub type_: ResponseOutputTextType,
    #[serde(default)]
    pub annotations: Vec<ResponseAnnotation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<Vec<TokenLogprob>>,
    pub text: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseOutputTextType {
    #[serde(rename = "output_text")]
    OutputText,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseRefusal {
    #[serde(rename = "type")]
    pub type_: ResponseRefusalType,
    pub refusal: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseRefusalType {
    #[serde(rename = "refusal")]
    Refusal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseReasoningText {
    #[serde(rename = "type")]
    pub type_: ResponseReasoningTextType,
    pub text: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseReasoningTextType {
    #[serde(rename = "reasoning_text")]
    ReasoningText,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputAudioContent {
    pub data: String,
    pub format: InputAudioFormat,
    #[serde(default, flatten)]
    pub rest: Rest,
}
