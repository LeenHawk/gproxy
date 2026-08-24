use serde::{Deserialize, Serialize};

use crate::openai::common::Rest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ApplyPatchOperation {
    #[serde(rename = "create_file")]
    CreateFile {
        diff: String,
        path: String,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "delete_file")]
    DeleteFile {
        path: String,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "update_file")]
    UpdateFile {
        diff: String,
        path: String,
        #[serde(default, flatten)]
        rest: Rest,
    },
}
