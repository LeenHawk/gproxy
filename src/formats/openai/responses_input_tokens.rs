use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use serde_valid::validation::Error as ValidationError;

use super::types::{
    ConversationParam, InputParam, Reasoning, ResponseTextParam, Tool, ToolChoiceParam, Truncation,
};

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
#[validate(custom = validate_responses_input_tokens_request)]
pub struct ResponseInputTokensRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub input: Option<InputParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub text: Option<ResponseTextParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Reasoning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncation: Option<Truncation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation: Option<ConversationParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoiceParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
}

fn validate_responses_input_tokens_request(
    req: &ResponseInputTokensRequest,
) -> Result<(), ValidationError> {
    if req.conversation.is_some() && req.previous_response_id.is_some() {
        return Err(ValidationError::Custom(
            "conversation and previous_response_id are mutually exclusive".to_string(),
        ));
    }

    if let Some(InputParam::Text(input)) = &req.input
        && input.len() > 10_485_760
    {
        return Err(ValidationError::Custom(
            "input must be at most 10485760 characters".to_string(),
        ));
    }

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseInputTokensResponse {
    #[serde(rename = "object")]
    pub object_type: ResponseInputTokensObjectType,
    pub input_tokens: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ResponseInputTokensObjectType {
    #[serde(rename = "response.input_tokens")]
    ResponseInputTokens,
}
