use serde::{Deserialize, Serialize};

use crate::openai::common::Rest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseAllowedTool {
    #[serde(rename = "function")]
    Function {
        name: String,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "custom")]
    Custom {
        name: String,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "mcp")]
    Mcp {
        server_label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "file_search")]
    FileSearch {
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "web_search_preview")]
    WebSearchPreview {
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "computer")]
    Computer {
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "computer_use_preview")]
    ComputerUsePreview {
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "computer_use")]
    ComputerUse {
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "web_search_preview_2025_03_11")]
    WebSearchPreview20250311 {
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "image_generation")]
    ImageGeneration {
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "code_interpreter")]
    CodeInterpreter {
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "local_shell")]
    LocalShell {
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "shell")]
    Shell {
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "apply_patch")]
    ApplyPatch {
        #[serde(default, flatten)]
        rest: Rest,
    },
}
