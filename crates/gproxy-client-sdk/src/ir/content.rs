use super::*;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Text {
    pub text: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub refusal: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub citations: Vec<Citation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Reasoning {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Content {
    Text(Text),
    Reasoning(Reasoning),
    Image(Image),
    Document(Document),
    File(File),
    Audio(Audio),
    Video(Video),
    ToolCall(ToolCall),
    ToolResult(ToolResult),
    Opaque,
    #[serde(untagged)]
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ToolResultContent {
    Text(Text),
    Image(Image),
    Document(Document),
    #[serde(untagged)]
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ChunkContent {
    Text(Text),
    Image(Image),
    #[serde(untagged)]
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ContentBlock<T> {
    #[serde(flatten)]
    pub content: T,
    #[serde(default)]
    pub provider_metadata: ProviderMetadata,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<CacheBreakpoint>,
}

pub type Block = ContentBlock<Content>;

pub type DocumentBlock = ContentBlock<Document>;

pub type DocumentChunk = ContentBlock<ChunkContent>;

pub type ToolResultBlock = ContentBlock<ToolResultContent>;

impl<T> ContentBlock<T> {
    pub fn new(content: T) -> Self {
        Self {
            content,
            provider_metadata: BTreeMap::new(),
            cache: None,
        }
    }
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> ContentBlock<U> {
        ContentBlock {
            content: f(self.content),
            provider_metadata: self.provider_metadata,
            cache: self.cache,
        }
    }
}

impl Block {
    pub fn text(text: impl Into<String>) -> Self {
        Self::new(Content::Text(Text {
            refusal: false,
            text: text.into(),
            citations: Vec::new(),
        }))
    }
}
