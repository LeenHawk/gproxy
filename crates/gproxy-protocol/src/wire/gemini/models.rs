use serde::{Deserialize, Serialize};

use crate::{Rest, WireRequest, WireResponse};

/// Request/response shapes for `models.list` and `models.get`.
/// `page_size` and `page_token` are query parameters; `name` is the
/// `models/{model}` path resource. Source: `upstream_docs/gemini/docs/Models.md`.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ListModelsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "page_size")]
    pub page_size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "page_token")]
    pub page_token: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

pub type ListModelsRequest = WireRequest<()>;
pub type GetModelRequest = WireRequest<()>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ListModelsResponseBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<Model>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "next_page_token")]
    pub next_page_token: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

pub type ListModelsResponse = WireResponse<ListModelsResponseBody>;
pub type GetModelResponse = WireResponse<Model>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct Model {
    pub name: String,
    #[serde(alias = "base_model_id")]
    pub base_model_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "display_name")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "input_token_limit")]
    pub input_token_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "output_token_limit")]
    pub output_token_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "supported_generation_methods")]
    pub supported_generation_methods: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "max_temperature")]
    pub max_temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "top_p")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "top_k")]
    pub top_k: Option<i64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
