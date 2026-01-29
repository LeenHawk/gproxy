use serde::{Deserialize, Serialize};

use crate::gemini::count_tokens::types::Content;
use crate::gemini::generate_content::types::{GenerationConfig, SafetySetting, Tool, ToolConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateContentPath {
    /// Format: models/{model}. It takes the form models/{model}.
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentRequestBody {
    /// Required. The content of the current conversation with the model.
    pub contents: Vec<Content>,
    /// Optional. Only used when GenerateContentRequestBody is embedded in countTokens
    /// (generateContentRequest). Normal generateContent uses the path model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_config: Option<ToolConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_settings: Option<Vec<SafetySetting>>,
    /// System instruction (text-only Content).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<GenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_content: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GenerateContentRequest {
    pub path: GenerateContentPath,
    pub body: GenerateContentRequestBody,
}
