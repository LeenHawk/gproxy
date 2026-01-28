use serde::{Deserialize, Serialize};

use crate::gemini::generate_content::types::{
    Candidate, ModelStatus, PromptFeedback, UsageMetadata,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentResponse {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<Candidate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_feedback: Option<PromptFeedback>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_metadata: Option<UsageMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_status: Option<ModelStatus>,
}
