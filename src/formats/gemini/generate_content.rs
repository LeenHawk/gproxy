use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use serde_valid::validation::Error as ValidationError;
use std::collections::HashSet;

use super::types::*;

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
#[validate(custom = validate_generate_content_request)]
pub struct GenerateContentRequest {
    #[serde(alias = "contents")]
    #[validate(min_items = 1)]
    #[validate]
    pub contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "tool_config")]
    #[validate]
    pub tool_config: Option<ToolConfig>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "safety_settings")]
    pub safety_settings: Option<Vec<SafetySetting>>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "system_instruction")]
    #[validate]
    pub system_instruction: Option<SystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "generation_config")]
    #[validate]
    pub generation_config: Option<GenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "cached_content")]
    pub cached_content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentResponse {
    pub candidates: Vec<Candidate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_feedback: Option<PromptFeedback>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_metadata: Option<UsageMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
}

fn validate_generate_content_request(req: &GenerateContentRequest) -> Result<(), ValidationError> {
    if let Some(settings) = &req.safety_settings {
        let mut seen = HashSet::new();
        for setting in settings {
            if !seen.insert(setting.category.clone()) {
                return Err(ValidationError::Custom(
                    "duplicate safetySettings category".to_string(),
                ));
            }
        }
    }
    Ok(())
}
