//! Thin generation/editing IR; native create_image/edit_image remain available.
pub use crate::media_support::{MediaStream, OperationUsage};
use crate::*;
use serde::{Deserialize, Serialize};
use std::{future::Future, num::NonZeroU32};
mod client;
mod dimensions;
pub mod gemini;
pub mod openai;
pub mod stream;
pub use stream::{ImageEvent, ImageStream};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ImageDialect {
    Openai,
    Gemini,
    #[serde(untagged)]
    Unknown(String),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ImageModel {
    pub id: String,
    pub dialect: ImageDialect,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AspectRatio {
    #[cfg_attr(feature = "ts", ts(as = "u32"))]
    pub width: NonZeroU32,
    #[cfg_attr(feature = "ts", ts(as = "u32"))]
    pub height: NonZeroU32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ImageSize {
    #[cfg_attr(feature = "ts", ts(as = "u32"))]
    pub width: NonZeroU32,
    #[cfg_attr(feature = "ts", ts(as = "u32"))]
    pub height: NonZeroU32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ResolutionTier {
    #[serde(rename = "512")]
    P512,
    #[serde(rename = "1K")]
    K1,
    #[serde(rename = "2K")]
    K2,
    #[serde(rename = "4K")]
    K4,
    #[serde(untagged)]
    Unknown(String),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ImageOutputFormat {
    Png,
    Jpeg,
    Webp,
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ImageOptions {
    pub openai: openai::OpenaiImageOptions,
    pub gemini: gemini::GeminiImageOptions,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ImageRequest {
    pub model: ImageModel,
    pub prompt: String,
    #[serde(default)]
    pub images: Vec<ContentBlock<Image>>,
    pub mask: Option<ContentBlock<Image>>,
    #[cfg_attr(feature = "ts", ts(as = "u32"))]
    pub n: NonZeroU32,
    pub aspect_ratio: Option<AspectRatio>,
    pub resolution: Option<ResolutionTier>,
    pub size: Option<ImageSize>,
    pub seed: Option<i64>,
    pub output_format: Option<ImageOutputFormat>,
    #[serde(default)]
    pub provider_options: ImageOptions,
}
impl ImageRequest {
    pub fn new(model: ImageModel, prompt: impl Into<String>) -> Self {
        Self {
            model,
            prompt: prompt.into(),
            images: Vec::new(),
            mask: None,
            n: NonZeroU32::MIN,
            aspect_ratio: None,
            resolution: None,
            size: None,
            seed: None,
            output_format: None,
            provider_options: ImageOptions::default(),
        }
    }
    pub async fn resolve_assets<F, Fut>(
        &self,
        mode: DeliveryMode,
        mut resolve: F,
    ) -> Result<PreparedImageRequest>
    where
        F: FnMut(ResolveAsset) -> Fut,
        Fut: Future<Output = Result<ResolvedAsset>>,
    {
        let target = self.target()?;
        self.check_inputs()?;
        let mut request = self.clone();
        let mut warnings = Vec::new();
        for (index, image) in request.images.iter_mut().enumerate() {
            crate::assets::resolve::prepare_media(
                &mut image.content.source,
                &mut image.content.media_type,
                target,
                MediaKind::Image,
                AssetLocation::ImageInput,
                &format!("images[{index}]"),
                mode,
                &mut resolve,
                &mut warnings,
            )
            .await?;
        }
        if let Some(mask) = &mut request.mask {
            crate::assets::resolve::prepare_media(
                &mut mask.content.source,
                &mut mask.content.media_type,
                target,
                MediaKind::Image,
                AssetLocation::ImageMask,
                "mask",
                mode,
                &mut resolve,
                &mut warnings,
            )
            .await?;
        }
        Ok(PreparedImageRequest { request, warnings })
    }
    pub(super) fn target(&self) -> Result<AssetDialect> {
        match self.model.dialect {
            ImageDialect::Openai => Ok(AssetDialect::OpenaiImages),
            ImageDialect::Gemini => Ok(AssetDialect::GeminiImages),
            ImageDialect::Unknown(_) => Err(Error::invalid(
                "image generation supports OpenAI and Gemini; Claude/unknown dialects are unsupported",
            )),
        }
    }
    pub(super) fn check_inputs(&self) -> Result<()> {
        if self.mask.is_some()
            && (self.images.is_empty() || self.model.dialect == ImageDialect::Gemini)
        {
            return Err(Error::invalid(
                "masks require OpenAI image editing with at least one input image",
            ));
        }
        Ok(())
    }
}
/// An ephemeral send snapshot. Persist the original application references.
pub struct PreparedImageRequest {
    pub(crate) request: ImageRequest,
    pub(crate) warnings: Vec<Warning>,
}
impl PreparedImageRequest {
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ImageResult {
    pub images: Vec<ContentBlock<Image>>,
    pub text: Vec<Block>,
    pub finish_reason: FinishReason,
    pub usage: OperationUsage,
    pub warnings: Vec<Warning>,
    pub response: ResponseMetadata,
    pub provider_metadata: ProviderMetadata,
}
