use serde::{Deserialize, Serialize};

use crate::claude::list_models::types::BetaModelInfo;
use crate::claude::types::RequestId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListModelsResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<RequestId>,
    pub data: Vec<BetaModelInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_id: Option<String>,
    pub has_more: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_id: Option<String>,
}
