use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BetaCountTokensContextManagementResponse {
    pub original_input_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BetaMessageTokensCount {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_management: Option<BetaCountTokensContextManagementResponse>,
    pub input_tokens: u32,
}

pub type CountTokensResponse = BetaMessageTokensCount;
