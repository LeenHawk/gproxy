use serde::{Deserialize, Serialize};

use crate::claude::types::RequestId;
use crate::claude::get_model::types::ModelInfo;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetModelResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<RequestId>,
    pub model: ModelInfo,
}
