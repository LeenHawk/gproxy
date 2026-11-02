//! Persist `Conversation`, not transport payloads. Its schema version is
//! an enum tag. Unknown schema versions are rejected; unknown enum variants
//! inside a known schema are retained for forward-compatible readers.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Vendor {
    Claude,
    Openai,
    Gemini,
    #[serde(untagged)]
    Unknown(String),
}

/// Available renderers. Vendor keys in persisted metadata are independent of
/// these features, so trimming a renderer cannot erase stored metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Dialect {
    #[cfg(feature = "claude")]
    Claude,
    #[cfg(feature = "openai")]
    Openai,
    #[cfg(feature = "gemini")]
    Gemini,
    #[cfg(feature = "chat")]
    OpenaiChat,
}

impl Dialect {
    pub fn vendor(self) -> Vendor {
        match self {
            #[cfg(feature = "claude")]
            Self::Claude => Vendor::Claude,
            #[cfg(feature = "openai")]
            Self::Openai => Vendor::Openai,
            #[cfg(feature = "gemini")]
            Self::Gemini => Vendor::Gemini,
            #[cfg(feature = "chat")]
            Self::OpenaiChat => Vendor::Openai,
        }
    }
    pub fn wire(self) -> gproxy_protocol::Dialect {
        match self {
            #[cfg(feature = "claude")]
            Self::Claude => gproxy_protocol::Dialect::Claude,
            #[cfg(feature = "openai")]
            Self::Openai => gproxy_protocol::Dialect::OpenAi,
            #[cfg(feature = "gemini")]
            Self::Gemini => gproxy_protocol::Dialect::Gemini,
            #[cfg(feature = "chat")]
            Self::OpenaiChat => gproxy_protocol::Dialect::OpenAiChat,
        }
    }
}

pub type ProviderMetadata = BTreeMap<Vendor, gproxy_protocol::Rest>;
pub use crate::options::ProviderOptions;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Model {
    pub id: String,
    pub dialect: Dialect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Asset {
    Inline {
        bytes: Vec<u8>,
    },
    Url {
        url: String,
    },
    Reference {
        reference: String,
    },
    ProviderFile {
        vendor: Vendor,
        id: String,
    },
    #[serde(untagged)]
    Unknown(Value),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Detail {
    Low,
    Medium,
    High,
    #[default]
    Auto,
    Original,
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum CacheTtl {
    FiveMinutes,
    ThirtyMinutes,
    OneHour,
    TwelveHours,
    #[serde(untagged)]
    Unknown(String),
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CacheBreakpoint {
    pub ttl: Option<CacheTtl>,
}

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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Image {
    pub source: Asset,
    pub media_type: Option<String>,
    #[serde(default)]
    pub detail: Detail,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum DocumentSource {
    Asset {
        source: Asset,
    },
    Text {
        text: String,
    },
    Chunks {
        chunks: Vec<DocumentChunk>,
    },
    #[serde(untagged)]
    Unknown(Value),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Document {
    pub source: DocumentSource,
    pub media_type: Option<String>,
    pub title: Option<String>,
    pub context: Option<String>,
    pub citations: Option<bool>,
    #[serde(default)]
    pub detail: Detail,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct File {
    pub source: Asset,
    pub media_type: Option<String>,
    pub filename: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum AudioFormat {
    Encoded {
        media_type: String,
    },
    Pcm {
        media_type: String,
        sample_rate: u32,
        channels: u16,
    },
    #[serde(untagged)]
    Unknown(Value),
}
impl AudioFormat {
    pub fn media_type(&self) -> Option<&str> {
        match self {
            Self::Encoded { media_type } | Self::Pcm { media_type, .. } => Some(media_type),
            Self::Unknown(_) => None,
        }
    }
    pub fn pcm(sample_rate: u32, channels: u16) -> Self {
        Self::Pcm {
            media_type: "audio/pcm".into(),
            sample_rate,
            channels,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Audio {
    pub source: Asset,
    pub format: AudioFormat,
    pub transcript: Option<String>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct VideoClip {
    pub start_seconds: Option<f64>,
    pub end_seconds: Option<f64>,
    pub fps: Option<f64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Video {
    pub source: Asset,
    pub media_type: Option<String>,
    pub clip: Option<VideoClip>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
    #[serde(default)]
    pub provider_executed: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ToolOutput {
    Json(Value),
    Blocks(Vec<ToolResultBlock>),
    #[serde(untagged)]
    Unknown(Value),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ToolResult {
    pub id: String,
    pub name: String,
    pub output: ToolOutput,
    pub is_error: bool,
    #[serde(default)]
    pub provider_executed: bool,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TextRange {
    pub start: u64,
    pub end: u64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum DocumentRange {
    Pages {
        start: u32,
        end: u32,
    },
    Chars {
        start: u64,
        end: u64,
    },
    Chunks {
        start: u32,
        end: u32,
    },
    #[serde(untagged)]
    Unknown(Value),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum DocumentReference {
    Block {
        document: Box<DocumentBlock>,
    },
    Index {
        vendor: Vendor,
        index: u32,
        file_id: Option<String>,
    },
    ProviderFile {
        vendor: Vendor,
        id: String,
    },
    #[serde(untagged)]
    Unknown(Value),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum CitationSource {
    Url {
        url: String,
    },
    Document {
        document: DocumentReference,
        range: Option<DocumentRange>,
    },
    #[serde(untagged)]
    Unknown(Value),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Citation {
    pub source: CitationSource,
    pub title: Option<String>,
    pub cited_text: Option<String>,
    pub text_range: Option<TextRange>,
    #[serde(default)]
    pub provider_metadata: ProviderMetadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Message {
    pub role: Role,
    pub content: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "schema_version")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Conversation {
    #[serde(rename = "1")]
    V1 { messages: Vec<Message> },
}
impl Conversation {
    pub fn new(messages: Vec<Message>) -> Self {
        Self::V1 { messages }
    }
    pub fn into_messages(self) -> Vec<Message> {
        match self {
            Self::V1 { messages } => messages,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ToolChoice {
    Auto,
    None,
    Required,
    Tool {
        name: String,
    },
    #[serde(untagged)]
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Modality {
    Text,
    Audio,
    Image,
    #[serde(untagged)]
    Unknown(String),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SpeechSettings {
    pub voice: String,
    pub format: AudioFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ReasoningEffort {
    ProviderDefault,
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    #[serde(untagged)]
    Unknown(String),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ReasoningSettings {
    pub effort: ReasoningEffort,
    pub token_budget: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Settings {
    #[serde(default)]
    pub reasoning: Option<ReasoningSettings>,
    #[serde(default)]
    pub output_modalities: Vec<Modality>,
    pub speech: Option<SpeechSettings>,
    pub max_output_tokens: Option<u32>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    #[serde(default)]
    pub stop_sequences: Vec<String>,
    pub tool_choice: Option<ToolChoice>,
    pub output_schema: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GenerateRequest {
    pub model: Model,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub tools: Vec<Tool>,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub provider_options: ProviderOptions,
}
impl GenerateRequest {
    pub fn new(model: Model, messages: Vec<Message>) -> Self {
        Self {
            model,
            messages,
            tools: Vec::new(),
            settings: Settings::default(),
            provider_options: ProviderOptions::default(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum FinishReasonKind {
    Stop,
    Length,
    ToolCalls,
    ContentBlocked,
    Error,
    Interrupted,
    #[default]
    Other,
    #[serde(untagged)]
    Unknown(String),
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FinishReason {
    pub unified: FinishReasonKind,
    pub raw: Option<String>,
}
impl FinishReason {
    pub fn new(unified: FinishReasonKind, raw: Option<String>) -> Self {
        Self { unified, raw }
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ResponseMetadata {
    pub id: Option<String>,
    pub model: Option<String>,
    /// Unix seconds reported by the provider, never a synthesized receipt time.
    pub timestamp: Option<i64>,
}
impl ResponseMetadata {
    pub(crate) fn merge(&mut self, update: Self) {
        if update.id.is_some() {
            self.id = update.id;
        }
        if update.model.is_some() {
            self.model = update.model;
        }
        if update.timestamp.is_some() {
            self.timestamp = update.timestamp;
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum WarningKind {
    Unsupported,
    Compatibility,
    Deprecated,
    Other,
    #[serde(untagged)]
    Unknown(String),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Warning {
    pub kind: WarningKind,
    pub field: String,
    pub message: String,
}
impl Warning {
    pub fn new(kind: WarningKind, field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind,
            field: field.into(),
            message: message.into(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Source {
    /// None when a retrieved snippet has no URL or file identity.
    pub source: Option<CitationSource>,
    pub title: Option<String>,
    pub text: Option<String>,
    #[serde(default)]
    pub provider_metadata: ProviderMetadata,
}

/// None means the provider did not report this counter; zero is an observed zero.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GenerateResult {
    #[serde(default)]
    pub response: ResponseMetadata,
    #[serde(default)]
    pub warnings: Vec<Warning>,
    #[serde(default)]
    pub sources: Vec<Source>,
    pub content: Vec<Block>,
    pub finish_reason: FinishReason,
    pub usage: Usage,
}
impl GenerateResult {
    pub fn into_message(self) -> Message {
        Message {
            role: Role::Assistant,
            content: self.content,
        }
    }
}

/// A native request and the diagnostics from compiling portable settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered<T> {
    pub request: T,
    pub warnings: Vec<Warning>,
}
