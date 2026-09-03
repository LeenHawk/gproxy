mod findings;
mod types;

use serde::{Deserialize, Serialize};

use crate::aws::Rest;

pub use findings::*;
pub use types::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GuardrailAutomatedReasoningPolicyAssessment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub findings: Option<Vec<GuardrailAutomatedReasoningFinding>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
