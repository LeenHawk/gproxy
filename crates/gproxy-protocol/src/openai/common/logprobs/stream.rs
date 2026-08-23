use serde::{Deserialize, Serialize};

use super::super::Rest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamTokenLogprob {
    pub token: String,
    pub logprob: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<Vec<StreamTokenTopLogprob>>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamTokenTopLogprob {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprob: Option<f64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}
