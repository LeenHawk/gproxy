use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::super::super::common::*;
use super::ResponseAnnotation;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseOutput {
    Text(String),
    Parts(Vec<ResponseToolOutputContentPart>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseEasyInputContent {
    Text(String),
    Parts(Vec<ResponseInputContentPart>),
    // `ResponseEasyInputMessageRole` accepts `assistant`, and replayed assistant history
    // (e.g. `codex exec resume`) carries `output_text` / `refusal` parts with no `id` or
    // `status` — so it lands here rather than in `ResponseOutputMessageItem`. Without this
    // variant every multi-turn Responses request fails to deserialize.
    OutputParts(Vec<ResponseMessageOutputContentPart>),
    // Last resort for arrays that match neither strict arm — i.e. a part tag this build
    // does not know, possibly mixed with parts it does. Keeping the raw values lets the
    // transform salvage the recognizable parts instead of dropping the whole message,
    // which is what a catch-all on the part enums cannot do without breaking the
    // untagged dispatch above.
    Raw(Vec<serde_json::Value>),
}

impl ResponseEasyInputContent {
    /// Best-effort text extraction from [`Self::Raw`]: pulls the text out of any part
    /// shape this build recognizes and silently skips the rest.
    pub fn raw_parts_text(parts: &[serde_json::Value]) -> String {
        parts
            .iter()
            .filter_map(|part| {
                let object = part.as_object()?;
                match object.get("type").and_then(serde_json::Value::as_str)? {
                    "input_text" | "text" | "output_text" | "summary_text"
                    | "reasoning_text" => Some(
                        object
                            .get("text")
                            .and_then(serde_json::Value::as_str)?
                            .to_owned(),
                    ),
                    "refusal" => Some(format!(
                        "[refusal: {}]",
                        object.get("refusal").and_then(serde_json::Value::as_str)?
                    )),
                    "encrypted_content" => Some("[encrypted content omitted]".to_owned()),
                    _ => None,
                }
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseInputContentPart {
    // `text` is the plain variant in the Responses spec; accept it as an alias so
    // clients that emit `{"type":"text"}` don't fail the whole request.
    #[serde(rename = "input_text", alias = "text")]
    InputText {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "input_image")]
    InputImage {
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<DetailLevel>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        image_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "input_file")]
    InputFile {
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<InputFileDetailLevel>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_data: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "input_audio")]
    InputAudio {
        input_audio: InputAudioContent,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    // NOTE: deliberately no `#[serde(other)]` fallback here. This enum is one of two
    // `Vec` arms of the untagged `ResponseEasyInputContent`; a catch-all would make
    // `Parts` match any array and swallow `output_text` into it, emptying assistant
    // history. Unknown tags are handled by `ResponseEasyInputContent::Raw` instead.
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseToolOutputContentPart {
    // Tool outputs on the wire mix codex-rs `FunctionCallOutputContentItem`
    // (input_text | input_image | encrypted_content) with legacy output blocks,
    // so accept `text` / `output_text` as aliases of the same text payload.
    #[serde(rename = "input_text", alias = "text", alias = "output_text")]
    InputText {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "input_image")]
    InputImage {
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<DetailLevel>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        image_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "input_file")]
    InputFile {
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<InputFileDetailLevel>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_data: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    // codex-rs `FunctionCallOutputContentItem::EncryptedContent`: an opaque
    // round-trip payload. Accepted so the request deserializes; dropped when
    // converting downstream since no non-OpenAI upstream can interpret it.
    #[serde(rename = "encrypted_content")]
    EncryptedContent {
        encrypted_content: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    // Legacy output block that can appear in tool results on the wire.
    #[serde(rename = "refusal")]
    Refusal {
        refusal: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    // See `ResponseInputContentPart::Unknown`.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseMessageOutputContentPart {
    #[serde(rename = "output_text", alias = "text")]
    OutputText {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        annotations: Vec<ResponseAnnotation>,
        #[serde(skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<TokenLogprob>>,
        text: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "refusal")]
    Refusal {
        refusal: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    // NOTE: no `#[serde(other)]` here either — same untagged-arm hazard as
    // `ResponseInputContentPart`, just in the opposite direction (`OutputParts`
    // would swallow `input_text`). See `ResponseEasyInputContent::Raw`.
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseOutputContentPart {
    #[serde(rename = "output_text")]
    OutputText {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        annotations: Vec<ResponseAnnotation>,
        #[serde(skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<TokenLogprob>>,
        text: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "refusal")]
    Refusal {
        refusal: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "reasoning_text")]
    ReasoningText {
        text: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
}

pub type ResponseContentPart = ResponseOutputContentPart;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputAudioContent {
    pub data: String,
    pub format: InputAudioFormat,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}
