use serde::{Deserialize, Serialize};

use crate::openai::common::Rest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MultiAgentConfig {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent_subagents: Option<u32>,
    #[serde(default, flatten)]
    pub rest: Rest,
}
