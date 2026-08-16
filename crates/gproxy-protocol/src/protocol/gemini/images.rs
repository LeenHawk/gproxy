//! Imagen native image generation wire models (Gemini API `:predict`).
//!
//! Imagen models (`imagen-*`) use this dedicated endpoint; `gemini-*-image`
//! models generate through `generateContent` instead and are covered by the
//! content-generation models.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::common::ExtraFields;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct ImagenPredictRequest {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instances: Vec<ImagenInstance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<ImagenParameters>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct ImagenInstance {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct ImagenParameters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub person_generation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub negative_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_options: Option<ImagenOutputOptions>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct ImagenOutputOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression_quality: Option<u32>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct ImagenPredictResponse {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub predictions: Vec<ImagenPrediction>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct ImagenPrediction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_base64_encoded: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rai_filtered_reason: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: ExtraFields,
}
