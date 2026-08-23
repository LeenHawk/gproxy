use serde::{Deserialize, Serialize};

use super::super::Rest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenLogprob {
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<Vec<u8>>,
    pub logprob: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top_logprobs: Vec<TokenLogprobTop>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenLogprobTop {
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<Vec<u8>>,
    pub logprob: f64,
    #[serde(default, flatten)]
    pub rest: Rest,
}
