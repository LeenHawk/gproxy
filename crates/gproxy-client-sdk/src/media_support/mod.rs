//! Shared transport and observed usage for the three media operation families.
use crate::*;
use serde::{Deserialize, Serialize};
#[cfg(any(feature = "images", feature = "transcription"))]
pub(crate) mod assets;
#[cfg(any(feature = "speech", feature = "transcription"))]
pub(crate) mod audio;
pub(crate) mod gemini;
pub(crate) mod io;
#[cfg(feature = "transcription")]
pub(crate) mod multipart;
pub use io::MediaStream;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OperationUsage {
    pub tokens: Usage,
    pub total_tokens: Option<u64>,
    pub characters: Option<u64>,
    pub audio_seconds: Option<f64>,
}
impl OperationUsage {
    #[cfg(any(feature = "speech", feature = "transcription"))]
    pub(crate) fn observe(&mut self, value: Self) {
        if value.tokens.input_tokens.is_some() {
            self.tokens.input_tokens = value.tokens.input_tokens;
        }
        if value.tokens.output_tokens.is_some() {
            self.tokens.output_tokens = value.tokens.output_tokens;
        }
        if value.tokens.reasoning_tokens.is_some() {
            self.tokens.reasoning_tokens = value.tokens.reasoning_tokens;
        }
        if value.tokens.cache_read_tokens.is_some() {
            self.tokens.cache_read_tokens = value.tokens.cache_read_tokens;
        }
        if value.tokens.cache_write_tokens.is_some() {
            self.tokens.cache_write_tokens = value.tokens.cache_write_tokens;
        }
        if value.total_tokens.is_some() {
            self.total_tokens = value.total_tokens;
        }
        if value.characters.is_some() {
            self.characters = value.characters;
        }
        if value.audio_seconds.is_some() {
            self.audio_seconds = value.audio_seconds;
        }
    }
    #[cfg(feature = "images")]
    pub(crate) fn add(&mut self, other: Self) -> Result<()> {
        fn sum(a: Option<u64>, b: Option<u64>) -> Result<Option<u64>> {
            a.zip(b)
                .map(|(a, b)| {
                    a.checked_add(b)
                        .ok_or_else(|| Error::invalid("usage overflow"))
                })
                .transpose()
        }
        self.tokens.input_tokens = sum(self.tokens.input_tokens, other.tokens.input_tokens)?;
        self.tokens.output_tokens = sum(self.tokens.output_tokens, other.tokens.output_tokens)?;
        self.tokens.reasoning_tokens =
            sum(self.tokens.reasoning_tokens, other.tokens.reasoning_tokens)?;
        self.tokens.cache_read_tokens = sum(
            self.tokens.cache_read_tokens,
            other.tokens.cache_read_tokens,
        )?;
        self.tokens.cache_write_tokens = sum(
            self.tokens.cache_write_tokens,
            other.tokens.cache_write_tokens,
        )?;
        self.total_tokens = sum(self.total_tokens, other.total_tokens)?;
        self.characters = sum(self.characters, other.characters)?;
        self.audio_seconds = self
            .audio_seconds
            .zip(other.audio_seconds)
            .map(|(a, b)| a + b);
        Ok(())
    }
}
pub(crate) fn count(value: i64) -> Result<u64> {
    value.try_into().map_err(Error::invalid)
}
pub(crate) fn optional_count(value: Option<i64>) -> Result<Option<u64>> {
    value.map(count).transpose()
}
pub(crate) fn metadata(value: impl Serialize) -> Result<protocol::Rest> {
    match serde_json::to_value(value).map_err(Error::invalid)? {
        serde_json::Value::Object(map) => Ok(map),
        _ => Err(Error::invalid("metadata must be an object")),
    }
}
pub(crate) fn warn(warnings: &mut Vec<Warning>, field: &str, message: &str) {
    warnings.push(Warning::new(WarningKind::Unsupported, field, message));
}
pub(crate) fn compatibility(warnings: &mut Vec<Warning>, field: &str, message: &str) {
    warnings.push(Warning::new(WarningKind::Compatibility, field, message));
}

#[cfg(any(feature = "images", feature = "speech"))]
pub(crate) fn native_metadata(value: impl Serialize) -> Result<protocol::Rest> {
    #[derive(Serialize)]
    struct Native<T> {
        native: T,
    }
    metadata(Native { native: value })
}
