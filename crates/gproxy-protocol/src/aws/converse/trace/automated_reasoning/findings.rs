use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::aws::{EmptyObject, Rest};

use super::types::{
    GuardrailAutomatedReasoningLogicWarning, GuardrailAutomatedReasoningRule,
    GuardrailAutomatedReasoningScenario, GuardrailAutomatedReasoningTranslation,
    GuardrailAutomatedReasoningTranslationOption,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum GuardrailAutomatedReasoningFinding {
    Impossible {
        impossible: GuardrailAutomatedReasoningImpossibleFinding,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Invalid {
        invalid: GuardrailAutomatedReasoningInvalidFinding,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    NoTranslations {
        #[serde(rename = "noTranslations")]
        no_translations: GuardrailAutomatedReasoningNoTranslationsFinding,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Satisfiable {
        satisfiable: GuardrailAutomatedReasoningSatisfiableFinding,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    TooComplex {
        #[serde(rename = "tooComplex")]
        too_complex: GuardrailAutomatedReasoningTooComplexFinding,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    TranslationAmbiguous {
        #[serde(rename = "translationAmbiguous")]
        translation_ambiguous: GuardrailAutomatedReasoningTranslationAmbiguousFinding,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Valid {
        valid: GuardrailAutomatedReasoningValidFinding,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GuardrailAutomatedReasoningImpossibleFinding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contradicting_rules: Option<Vec<GuardrailAutomatedReasoningRule>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logic_warning: Option<GuardrailAutomatedReasoningLogicWarning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translation: Option<GuardrailAutomatedReasoningTranslation>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GuardrailAutomatedReasoningInvalidFinding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contradicting_rules: Option<Vec<GuardrailAutomatedReasoningRule>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logic_warning: Option<GuardrailAutomatedReasoningLogicWarning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translation: Option<GuardrailAutomatedReasoningTranslation>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

pub type GuardrailAutomatedReasoningNoTranslationsFinding = EmptyObject;
pub type GuardrailAutomatedReasoningTooComplexFinding = EmptyObject;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GuardrailAutomatedReasoningSatisfiableFinding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claims_false_scenario: Option<GuardrailAutomatedReasoningScenario>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claims_true_scenario: Option<GuardrailAutomatedReasoningScenario>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logic_warning: Option<GuardrailAutomatedReasoningLogicWarning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translation: Option<GuardrailAutomatedReasoningTranslation>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GuardrailAutomatedReasoningTranslationAmbiguousFinding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub difference_scenarios: Option<Vec<GuardrailAutomatedReasoningScenario>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<GuardrailAutomatedReasoningTranslationOption>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GuardrailAutomatedReasoningValidFinding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claims_true_scenario: Option<GuardrailAutomatedReasoningScenario>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logic_warning: Option<GuardrailAutomatedReasoningLogicWarning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supporting_rules: Option<Vec<GuardrailAutomatedReasoningRule>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translation: Option<GuardrailAutomatedReasoningTranslation>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
