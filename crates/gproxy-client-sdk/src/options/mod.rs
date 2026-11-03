//! SDK-owned provider options; portable controls live in Settings.
use serde::{Deserialize, Serialize};
mod claude;
mod gemini;
mod openai;
pub use claude::*;
pub use gemini::*;
pub use openai::*;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ProviderOptions {
    #[serde(default)]
    pub claude: ClaudeOptions,
    #[serde(default)]
    pub openai: OpenaiOptions,
    #[serde(default)]
    pub gemini: GeminiOptions,
}

impl ProviderOptions {
    pub fn new() -> Self {
        Self::default()
    }
    #[cfg(any(
        all(
            feature = "count-tokens",
            any(feature = "claude", feature = "openai", feature = "gemini")
        ),
        feature = "embeddings",
        feature = "rerank",
        feature = "audio",
        feature = "images",
        feature = "video"
    ))]
    pub(crate) fn remainder(
        &self,
        vendor: &crate::Vendor,
    ) -> crate::Result<&gproxy_protocol::Rest> {
        match vendor {
            crate::Vendor::Claude => Ok(&self.claude.rest),
            crate::Vendor::Openai => Ok(&self.openai.rest),
            crate::Vendor::Gemini => Ok(&self.gemini.rest),
            crate::Vendor::Unknown(_) => Err(crate::Error::invalid(
                "unknown vendor options cannot be sent",
            )),
        }
    }
}

pub(crate) fn present_nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}
