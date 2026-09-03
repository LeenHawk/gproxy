use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::aws::{DocumentFormat, ImageFormat, Rest};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ImageBlock {
    pub format: ImageFormat,
    pub source: ImageSource,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ImageSource {
    Bytes {
        bytes: String,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    S3Location {
        #[serde(rename = "s3Location")]
        s3_location: S3Location,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct DocumentBlock {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<DocumentFormat>,
    pub name: String,
    pub source: DocumentSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum DocumentSource {
    Bytes {
        bytes: String,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    S3Location {
        #[serde(rename = "s3Location")]
        s3_location: S3Location,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Text {
        text: String,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct S3Location {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bucket_owner: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
