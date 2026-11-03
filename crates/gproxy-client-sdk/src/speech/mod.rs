//! Speech synthesis IR. Client::speech keeps its native passthrough signature.
pub use crate::media_support::{MediaStream, OperationUsage};
use crate::*;
use serde::{Deserialize, Serialize};
mod client;
pub mod gemini;
pub mod openai;
mod state;
pub mod stream;
pub use stream::{SpeechEvent, SpeechStream};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum SpeechDialect {
    Openai,
    Gemini,
    #[serde(untagged)]
    Unknown(String),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SpeechModel {
    pub id: String,
    pub dialect: SpeechDialect,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SpeechOptions {
    pub openai: openai::OpenaiSpeechOptions,
    pub gemini: gemini::GeminiSpeechOptions,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SpeechRequest {
    pub model: SpeechModel,
    pub text: String,
    pub voice: Option<String>,
    pub instructions: Option<String>,
    pub speed: Option<f64>,
    pub language: Option<String>,
    pub format: AudioFormat,
    #[serde(default)]
    pub provider_options: SpeechOptions,
}
impl SpeechRequest {
    pub fn new(model: SpeechModel, text: impl Into<String>) -> Self {
        Self {
            model,
            text: text.into(),
            voice: None,
            instructions: None,
            speed: None,
            language: None,
            format: AudioFormat::Encoded {
                media_type: "audio/mpeg".into(),
            },
            provider_options: SpeechOptions::default(),
        }
    }
    pub(super) fn check(&self) -> Result<()> {
        if matches!(self.model.dialect, SpeechDialect::Unknown(_)) {
            return Err(Error::invalid(
                "speech synthesis supports OpenAI and Gemini; Claude/unknown dialects are unsupported",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SpeechResult {
    pub audio: Option<ContentBlock<Audio>>,
    pub finish_reason: FinishReason,
    pub usage: OperationUsage,
    pub warnings: Vec<Warning>,
    pub response: ResponseMetadata,
    pub provider_metadata: ProviderMetadata,
}
