use serde::{Deserialize, Serialize};

use crate::claude::types::RequestId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BetaCountTokensContextManagementResponse {
    pub original_input_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BetaMessageTokensCount {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<RequestId>,
    pub context_management: BetaCountTokensContextManagementResponse,
    pub input_tokens: u32,
}

pub type CountTokensResponse = BetaMessageTokensCount;
