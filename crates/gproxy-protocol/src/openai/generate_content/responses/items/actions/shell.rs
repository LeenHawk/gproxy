use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::openai::common::Rest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct LocalShellAction {
    pub command: Vec<String>,
    pub env: BTreeMap<String, String>,
    #[serde(rename = "type")]
    pub type_: LocalShellActionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum LocalShellActionType {
    #[serde(rename = "exec")]
    Exec,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ShellAction {
    pub commands: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ShellEnvironment {
    #[serde(rename = "local")]
    Local {
        #[serde(skip_serializing_if = "Option::is_none")]
        skills: Option<Vec<ShellSkillReference>>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "container_reference")]
    ContainerReference {
        container_id: String,
        #[serde(default, flatten)]
        rest: Rest,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ShellSkillReference {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ShellCallOutputContent {
    pub outcome: ShellCallOutcome,
    pub stderr: String,
    pub stdout: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ShellCallOutcome {
    #[serde(rename = "timeout")]
    Timeout {
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "exit")]
    Exit {
        exit_code: i32,
        #[serde(default, flatten)]
        rest: Rest,
    },
}
