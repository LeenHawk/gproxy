use serde::{Deserialize, Serialize};

use crate::openai::create_response::types::{
    ConversationParam, InputParam, Reasoning, ResponseTextParam, Tool, ToolChoiceParam, Truncation,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InputTokenCountRequestBody {
    /// Model ID used to generate the response.
    pub model: String,
    /// Text, image, or file inputs to the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<InputParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<ResponseTextParam>,
    /// Reasoning config (reasoning models only; not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Reasoning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncation: Option<Truncation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// Cannot be used together with `previous_response_id` (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation: Option<ConversationParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoiceParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct InputTokenCountRequest {
    pub body: InputTokenCountRequestBody,
}
