use super::*;

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
