use serde::{Deserialize, Serialize};

use crate::openai::common::Rest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct MultiAgentConfig {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent_subagents: Option<u32>,
    #[serde(default, flatten)]
    pub rest: Rest,
}
