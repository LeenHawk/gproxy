mod automated_reasoning;
mod filters;
mod policies;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::aws::Rest;

pub use automated_reasoning::*;
pub use filters::*;
pub use policies::*;

/// AWS `ConverseTrace` response object (`Converse.md` trace).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConverseTrace {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guardrail: Option<GuardrailTraceAssessment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_router: Option<PromptRouterTrace>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// AWS names the streaming object separately but documents the same shape.
pub type ConverseStreamTrace = ConverseTrace;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptRouterTrace {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoked_model_id: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailTraceAssessment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_assessment: Option<BTreeMap<String, GuardrailAssessment>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_output: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_assessments: Option<BTreeMap<String, Vec<GuardrailAssessment>>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailAssessment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_guardrail_details: Option<AppliedGuardrailDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automated_reasoning_policy: Option<GuardrailAutomatedReasoningPolicyAssessment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_policy: Option<GuardrailContentPolicyAssessment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contextual_grounding_policy: Option<GuardrailContextualGroundingPolicyAssessment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invocation_metrics: Option<GuardrailInvocationMetrics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensitive_information_policy: Option<GuardrailSensitiveInformationPolicyAssessment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic_policy: Option<GuardrailTopicPolicyAssessment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub word_policy: Option<GuardrailWordPolicyAssessment>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
