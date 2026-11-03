use super::*;
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub(super) enum ReasoningMetadata {
    #[serde(rename = "thinking")]
    Thinking {
        signature: String,
        #[serde(flatten)]
        rest: Rest,
    },
    #[serde(rename = "redacted_thinking")]
    Redacted {
        data: String,
        #[serde(flatten)]
        rest: Rest,
    },
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct TextMetadata {
    pub(super) empty_citations: bool,
    #[serde(flatten)]
    pub(super) rest: Rest,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct ToolMetadata {
    pub(super) caller: Option<c::Caller>,
    pub(super) toolset_name: Option<String>,
    #[serde(flatten)]
    pub(super) rest: Rest,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct ImageMetadata {
    pub(super) source_rest: Rest,
    #[serde(flatten)]
    pub(super) rest: Rest,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct DocumentMetadata {
    pub(super) source_rest: Rest,
    pub(super) chunk_text: bool,
    pub(super) citations: Option<c::CitationsConfig>,
    #[serde(flatten)]
    pub(super) rest: Rest,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct ToolResultMetadata {
    pub(super) toolset_name: Option<String>,
    pub(super) is_error_present: bool,
    pub(super) native_blocks: bool,
    #[serde(flatten)]
    pub(super) rest: Rest,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ServerResultKind {
    Search,
    Fetch,
    Code,
    Bash,
    Edit,
    Tools,
    Advisor,
}

#[derive(Serialize, Deserialize)]
pub(super) struct ServerResultMetadata {
    pub(super) kind: ServerResultKind,
    pub(super) caller: Option<c::Caller>,
    #[serde(default, flatten)]
    pub(super) rest: Rest,
}
