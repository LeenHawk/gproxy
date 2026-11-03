use super::*;
#[derive(Default, Serialize, Deserialize)]
pub(super) struct Envelope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) chat: Option<Metadata>,
}

#[derive(Default, Serialize, Deserialize)]
pub(super) struct Metadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) message: Option<MessageMetadata>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(super) legacy: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) call_type: Option<c::ChatToolCallType>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub(super) rest: Rest,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub(super) function_rest: Rest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) audio: Option<AudioMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) reasoning_source: Option<ReasoningSource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) reasoning_layout: Vec<ReasoningSegment>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct ReasoningSegment {
    pub(super) index: usize,
    pub(super) chars: usize,
}

#[derive(Default, Serialize, Deserialize)]
pub(super) struct MessageMetadata {
    #[serde(
        default,
        deserialize_with = "crate::options::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) reasoning_details:
        Option<Option<Vec<gproxy_protocol::openai::chat::reasoning::ReasoningDetail>>>,
    #[serde(
        default,
        deserialize_with = "crate::options::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) reasoning_content: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::options::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) reasoning: Option<Option<String>>,
    #[serde(default, flatten)]
    pub(super) rest: Rest,
}

#[derive(Default, Serialize, Deserialize)]
pub(super) struct AudioMetadata {
    pub(super) id: Option<String>,
    pub(super) expires_at: Option<i64>,
    #[serde(default, flatten)]
    pub(super) rest: Rest,
}

// These are extensions in protocol's current Delta.rest. Decode that remainder
// once at its boundary; the mapping never indexes the native Delta as JSON.
#[derive(Default, Deserialize)]
pub(super) struct StreamExtensions {
    pub(super) audio: Option<AudioDelta>,
    pub(super) annotations: Option<Vec<r::Annotation>>,
    #[serde(default, flatten)]
    pub(super) rest: Rest,
}

#[derive(Deserialize)]
pub(super) struct AudioDelta {
    pub(super) id: Option<String>,
    pub(super) expires_at: Option<i64>,
    pub(super) data: Option<String>,
    pub(super) transcript: Option<String>,
    #[serde(default, flatten)]
    pub(super) rest: Rest,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ReasoningSource {
    Content,
    Reasoning,
    Details,
}

pub(super) fn metadata(block: &ContentBlock<impl Sized>) -> Result<Option<Metadata>> {
    Ok(read_metadata::<Envelope>(block, Vendor::Openai)?.and_then(|v| v.chat))
}

pub(super) fn block(content: Content, meta: Metadata) -> Result<Block> {
    with_metadata(content, Vendor::Openai, Envelope { chat: Some(meta) })
}
