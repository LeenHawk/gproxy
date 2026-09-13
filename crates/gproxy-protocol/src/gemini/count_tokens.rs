use super::{
    content::{Content, Modality},
    generation::EmbeddedGenerateContentRequest,
};
use crate::Rest;
use crate::{WireRequest, WireResponse};
use serde::{Deserialize, Serialize};

/// Body for `POST .../{model=models/*}:countTokens`; `model` is the path resource.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CountTokensRequestBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contents: Option<Vec<Content>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "generate_content_request")]
    pub generate_content_request: Option<EmbeddedGenerateContentRequest>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ModalityTokenCount {
    pub modality: Modality,
    #[serde(alias = "token_count")]
    pub token_count: i64,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CountTokensResponseBody {
    #[serde(alias = "total_tokens")]
    pub total_tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "cached_content_token_count")]
    pub cached_content_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "prompt_tokens_details")]
    pub prompt_tokens_details: Option<Vec<ModalityTokenCount>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "cache_tokens_details")]
    pub cache_tokens_details: Option<Vec<ModalityTokenCount>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

pub type CountTokensRequest = WireRequest<CountTokensRequestBody>;
pub type CountTokensResponse = WireResponse<CountTokensResponseBody>;
