use serde::{Deserialize, Serialize};

use crate::aws::{GuardrailAutomatedReasoningLogicWarningType, Rest};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailAutomatedReasoningRule {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_version_arn: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailAutomatedReasoningLogicWarning {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claims: Option<Vec<GuardrailAutomatedReasoningStatement>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premises: Option<Vec<GuardrailAutomatedReasoningStatement>>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<GuardrailAutomatedReasoningLogicWarningType>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailAutomatedReasoningTranslation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claims: Option<Vec<GuardrailAutomatedReasoningStatement>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premises: Option<Vec<GuardrailAutomatedReasoningStatement>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub untranslated_claims: Option<Vec<GuardrailAutomatedReasoningInputTextReference>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub untranslated_premises: Option<Vec<GuardrailAutomatedReasoningInputTextReference>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardrailAutomatedReasoningScenario {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statements: Option<Vec<GuardrailAutomatedReasoningStatement>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuardrailAutomatedReasoningTranslationOption {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translations: Option<Vec<GuardrailAutomatedReasoningTranslation>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailAutomatedReasoningStatement {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub natural_language: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardrailAutomatedReasoningInputTextReference {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
