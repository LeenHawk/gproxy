use super::*;

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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Message {
    pub role: Role,
    pub content: Vec<Block>,
}

#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    feature = "realtime"
))]
impl Message {
    pub(crate) fn model_content(&self) -> impl Iterator<Item = &Block> {
        self.content
            .iter()
            .filter(|b| !matches!(b.content, Content::Interrupted(_)))
    }
    pub(crate) fn is_record_only(&self) -> bool {
        !self.content.is_empty() && self.model_content().next().is_none()
    }
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
