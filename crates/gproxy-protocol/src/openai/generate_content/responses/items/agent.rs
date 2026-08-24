use serde::{Deserialize, Serialize};

use crate::openai::common::Rest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum AgentMessageContentPart {
    #[serde(rename = "encrypted_content")]
    EncryptedContent {
        encrypted_content: String,
        #[serde(default, flatten)]
        rest: Rest,
    },
}
