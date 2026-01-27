use serde::{Deserialize, Serialize};

use crate::gemini::count_tokens::types::{Content, GenerateContentRequest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountTokensPath {
    /// Format: models/{model}. It takes the form models/{model}.
    pub model: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CountTokensRequestBody {
    /// Mutually exclusive with generateContentRequest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contents: Option<Vec<Content>>,
    /// Mutually exclusive with contents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generate_content_request: Option<GenerateContentRequest>,
}

#[derive(Debug, Clone)]
pub struct CountTokensRequest {
    pub path: CountTokensPath,
    pub body: CountTokensRequestBody,
}
