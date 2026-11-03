//! Portable embedding requests and results; native execution lives by dialect.
use crate::*;
use serde::{Deserialize, Serialize};
use std::num::{NonZeroU32, NonZeroUsize};
mod client;
mod gemini;
mod openai;
pub use gemini::GeminiEmbeddingOptions;
pub use openai::OpenaiEmbeddingOptions;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum EmbeddingDialect {
    Openai,
    Gemini,
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EmbeddingModel {
    pub id: String,
    pub dialect: EmbeddingDialect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum EmbeddingInput {
    Text(String),
    Image(Image),
    Parts(Vec<EmbeddingPart>),
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum EmbeddingPart {
    Text(String),
    Image(Image),
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum EmbeddingPurpose {
    Query,
    Document,
    SemanticSimilarity,
    Classification,
    Clustering,
    QuestionAnswering,
    FactVerification,
    CodeQuery,
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EmbeddingOptions {
    pub openai: OpenaiEmbeddingOptions,
    pub gemini: GeminiEmbeddingOptions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EmbedRequest {
    pub model: EmbeddingModel,
    pub inputs: Vec<EmbeddingInput>,
    #[cfg_attr(feature = "ts", ts(as = "Option<u32>"))]
    pub dimensions: Option<NonZeroU32>,
    pub purpose: Option<EmbeddingPurpose>,
    /// Upper bound on native inputs per call. OpenAI packs at most 2048;
    /// Gemini uses one routed embedContent call per input.
    #[cfg_attr(feature = "ts", ts(as = "Option<usize>"))]
    pub batch_size: Option<NonZeroUsize>,
    #[serde(default)]
    pub provider_options: EmbeddingOptions,
}

impl EmbedRequest {
    pub fn new(model: EmbeddingModel, inputs: Vec<EmbeddingInput>) -> Self {
        Self {
            model,
            inputs,
            dimensions: None,
            purpose: None,
            batch_size: None,
            provider_options: EmbeddingOptions::default(),
        }
    }
    pub async fn resolve_assets<F, Fut>(
        &self,
        mode: DeliveryMode,
        mut resolve: F,
    ) -> Result<PreparedEmbedRequest>
    where
        F: FnMut(ResolveAsset) -> Fut,
        Fut: std::future::Future<Output = Result<ResolvedAsset>>,
    {
        let mut request = self.clone();
        let target = match request.model.dialect {
            EmbeddingDialect::Openai => AssetDialect::OpenaiEmbedding,
            EmbeddingDialect::Gemini => AssetDialect::GeminiEmbedding,
            EmbeddingDialect::Unknown(_) => {
                return Err(Error::invalid("unknown embedding dialect"));
            }
        };
        let mut warnings = Vec::new();
        for (i, input) in request.inputs.iter_mut().enumerate() {
            match input {
                EmbeddingInput::Image(image) => {
                    crate::assets::resolve::prepare_image(
                        image,
                        target,
                        AssetLocation::Embedding,
                        &format!("inputs[{i}]"),
                        mode,
                        &mut resolve,
                        &mut warnings,
                    )
                    .await?
                }
                EmbeddingInput::Parts(parts) => {
                    for (j, part) in parts.iter_mut().enumerate() {
                        if let EmbeddingPart::Image(image) = part {
                            crate::assets::resolve::prepare_image(
                                image,
                                target,
                                AssetLocation::Embedding,
                                &format!("inputs[{i}].parts[{j}]"),
                                mode,
                                &mut resolve,
                                &mut warnings,
                            )
                            .await?;
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(PreparedEmbedRequest { request, warnings })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EmbedResult {
    pub vectors: Vec<Vec<f64>>,
    pub usage: Usage,
    pub warnings: Vec<Warning>,
}

/// Send-only embedding snapshot; resolved URLs cannot be serialized as history.
pub struct PreparedEmbedRequest {
    request: EmbedRequest,
    warnings: Vec<Warning>,
}

impl PreparedEmbedRequest {
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }
}
