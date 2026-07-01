use serde::{Deserialize, Serialize};

use super::super::JsonObject;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserLocation {
    #[serde(rename = "type")]
    pub type_: UserLocationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserLocationType {
    #[serde(rename = "approximate")]
    Approximate,
}
