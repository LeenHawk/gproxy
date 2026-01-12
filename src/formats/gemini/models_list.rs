use serde::{Deserialize, Serialize};
use serde_valid::Validate;

use super::types::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsListRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<ModelsListQuery>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ModelsListQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(minimum = 1)]
    #[validate(maximum = 1000)]
    pub page_size: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsListResponse {
    pub models: Vec<Model>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}
