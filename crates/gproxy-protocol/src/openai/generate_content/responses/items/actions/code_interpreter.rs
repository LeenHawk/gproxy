use serde::{Deserialize, Serialize};

use crate::openai::common::Rest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum CodeInterpreterOutput {
    #[serde(rename = "logs")]
    Logs {
        logs: String,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "image")]
    Image {
        url: String,
        #[serde(default, flatten)]
        rest: Rest,
    },
}
