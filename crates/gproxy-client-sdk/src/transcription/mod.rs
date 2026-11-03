//! Transcription IR. Client::transcribe preserves the native passthrough API.
pub use crate::media_support::{MediaStream, OperationUsage};
use crate::*;
use serde::{Deserialize, Serialize};
use std::future::Future;
mod client;
pub mod gemini;
pub mod openai;
pub mod stream;
pub use stream::{TranscriptionEvent, TranscriptionStream};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum TranscriptionDialect {
    Openai,
    Gemini,
    #[serde(untagged)]
    Unknown(String),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TranscriptionModel {
    pub id: String,
    pub dialect: TranscriptionDialect,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum TimestampGranularity {
    Segment,
    Word,
    #[serde(untagged)]
    Unknown(String),
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TranscriptionOptions {
    pub openai: openai::OpenaiTranscriptionOptions,
    pub gemini: gemini::GeminiTranscriptionOptions,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TranscriptionRequest {
    pub model: TranscriptionModel,
    pub audio: ContentBlock<Audio>,
    pub language: Option<String>,
    pub prompt: Option<String>,
    #[serde(default)]
    pub timestamps: Vec<TimestampGranularity>,
    #[serde(default)]
    pub diarization: bool,
    #[serde(default)]
    pub provider_options: TranscriptionOptions,
}
impl TranscriptionRequest {
    pub fn new(model: TranscriptionModel, audio: ContentBlock<Audio>) -> Self {
        Self {
            model,
            audio,
            language: None,
            prompt: None,
            timestamps: Vec::new(),
            diarization: false,
            provider_options: TranscriptionOptions::default(),
        }
    }
    pub async fn resolve_assets<F, Fut>(
        &self,
        mode: DeliveryMode,
        mut resolve: F,
    ) -> Result<PreparedTranscriptionRequest>
    where
        F: FnMut(ResolveAsset) -> Fut,
        Fut: Future<Output = Result<ResolvedAsset>>,
    {
        let target = self.target()?;
        let mut request = self.clone();
        let mut warnings = Vec::new();
        let mut mime = Some(
            request
                .audio
                .content
                .format
                .media_type()
                .ok_or_else(|| Error::invalid("transcription input has an unknown audio format"))?
                .to_owned(),
        );
        crate::assets::resolve::prepare_media(
            &mut request.audio.content.source,
            &mut mime,
            target,
            MediaKind::Audio,
            AssetLocation::Transcription,
            "audio",
            mode,
            &mut resolve,
            &mut warnings,
        )
        .await?;
        Ok(PreparedTranscriptionRequest { request, warnings })
    }
    pub(super) fn target(&self) -> Result<AssetDialect> {
        match self.model.dialect {
            TranscriptionDialect::Openai => Ok(AssetDialect::OpenaiTranscription),
            TranscriptionDialect::Gemini => Ok(AssetDialect::GeminiTranscription),
            TranscriptionDialect::Unknown(_) => Err(Error::invalid(
                "transcription supports OpenAI and Gemini; Claude/unknown dialects are unsupported",
            )),
        }
    }
}
pub struct PreparedTranscriptionRequest {
    pub(crate) request: TranscriptionRequest,
    pub(crate) warnings: Vec<Warning>,
}
impl PreparedTranscriptionRequest {
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TranscriptSegment {
    pub text: String,
    pub start: Option<f64>,
    pub end: Option<f64>,
    pub speaker: Option<String>,
    #[serde(default)]
    pub provider_metadata: ProviderMetadata,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TranscriptWord {
    pub text: String,
    pub start: Option<f64>,
    pub end: Option<f64>,
    pub speaker: Option<String>,
    #[serde(default)]
    pub provider_metadata: ProviderMetadata,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TranscriptionResult {
    pub text: String,
    pub segments: Vec<TranscriptSegment>,
    pub words: Vec<TranscriptWord>,
    pub language: Option<String>,
    pub duration: Option<f64>,
    pub finish_reason: FinishReason,
    pub usage: OperationUsage,
    pub warnings: Vec<Warning>,
    pub response: ResponseMetadata,
    pub provider_metadata: ProviderMetadata,
}
impl TranscriptionResult {
    pub(super) fn empty(warnings: Vec<Warning>) -> Self {
        Self {
            text: String::new(),
            segments: Vec::new(),
            words: Vec::new(),
            language: None,
            duration: None,
            finish_reason: crate::media_support::io::interrupted(),
            usage: OperationUsage::default(),
            warnings,
            response: ResponseMetadata::default(),
            provider_metadata: ProviderMetadata::default(),
        }
    }
}
