use serde::{Deserialize, Serialize};

use super::{Content, JsonMap, Tool, ToolConfig};

pub type CreateCachedContentRequest = CachedContent;
pub type CreateCachedContentResponse = CachedContent;
pub type GetCachedContentResponse = CachedContent;
pub type UpdateCachedContentRequest = CachedContent;
pub type UpdateCachedContentResponse = CachedContent;
pub type DeleteCachedContentResponse = JsonMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CachedContent {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contents: Vec<Content>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Tool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_metadata: Option<CachedContentUsageMetadata>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub expiration: Option<CachedContentExpiration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_config: Option<ToolConfig>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum CachedContentExpiration {
    ExpireTime {
        #[serde(rename = "expireTime")]
        expire_time: String,
    },
    Ttl {
        ttl: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CachedContentUsageMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_token_count: Option<i32>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ListCachedContentsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ListCachedContentsResponse {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cached_contents: Vec<CachedContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GetCachedContentRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct UpdateCachedContentQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_mask: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

pub type DeleteCachedContentRequest = GetCachedContentRequest;

pub type CreateCachedContentRequestBody = CreateCachedContentRequest;
pub type CreateCachedContentResponseBody = CreateCachedContentResponse;
pub type GetCachedContentResponseBody = GetCachedContentResponse;
pub type UpdateCachedContentRequestBody = UpdateCachedContentRequest;
pub type UpdateCachedContentResponseBody = UpdateCachedContentResponse;
pub type DeleteCachedContentResponseBody = DeleteCachedContentResponse;
pub type ListCachedContentsResponseBody = ListCachedContentsResponse;
