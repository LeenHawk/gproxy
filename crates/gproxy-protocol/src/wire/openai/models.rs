use serde::{Deserialize, Serialize};

use crate::Rest;

/// Query/path request shapes for `GET /models` and `GET /models/{model}`.
/// Source: `upstream_docs/openai/docs/List models.md` and `Retrieve model.md`.
pub type ListModelsRequest = crate::WireRequest<()>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ListObject {
    List,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ModelObject {
    Model,
}

pub type GetModelRequest = crate::WireRequest<()>;

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ListModelsResponseBody {
    pub data: Vec<Model>,
    pub object: ListObject,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

pub type ListModelsResponse = crate::WireResponse<ListModelsResponseBody>;
pub type GetModelResponse = crate::WireResponse<Model>;

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct Model {
    pub id: String,
    pub created: i64,
    pub object: ModelObject,
    pub owned_by: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
