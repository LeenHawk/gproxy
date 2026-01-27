use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InputTokenObjectType {
    #[serde(rename = "response.input_tokens")]
    ResponseInputTokens,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InputTokenCount {
    pub object: InputTokenObjectType,
    pub input_tokens: i64,
}
