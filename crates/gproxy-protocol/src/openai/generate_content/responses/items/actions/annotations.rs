use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::Rest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseAnnotation {
    #[serde(rename = "file_citation")]
    FileCitation {
        file_id: String,
        filename: String,
        index: u32,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "url_citation")]
    UrlCitation {
        end_index: u32,
        start_index: u32,
        title: String,
        url: String,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "container_file_citation")]
    ContainerFileCitation {
        container_id: String,
        end_index: u32,
        file_id: String,
        filename: String,
        start_index: u32,
        #[serde(default, flatten)]
        rest: Rest,
    },
    #[serde(rename = "file_path")]
    FilePath {
        file_id: String,
        index: u32,
        #[serde(default, flatten)]
        rest: Rest,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FileSearchResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<FileSearchResultAttributes>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

pub type FileSearchResultAttributes = BTreeMap<String, FileSearchResultAttributeValue>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum FileSearchResultAttributeValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Unknown(Value),
}
