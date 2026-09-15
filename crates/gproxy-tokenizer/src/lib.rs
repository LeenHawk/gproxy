//! Local token counting, with no network, persistence or task runtime.
//!
//! Callers select and retain a tokenizer. Request counting encodes visible text
//! without adding message overhead or applying a model's chat template.

#![doc = include_str!("../README.md")]

mod error;
mod extract;

pub use error::CountError;
pub use extract::RequestFormat;

/// A reusable local encoder. Model selection and fallback policy belong to callers.
pub struct Tokenizer {
    backend: Backend,
}

enum Backend {
    CharacterEstimate,
    #[cfg(feature = "tiktoken")]
    Tiktoken(&'static tiktoken_rs::CoreBPE),
    #[cfg(feature = "huggingface")]
    HuggingFace(Box<tokenizers::Tokenizer>),
}

impl Tokenizer {
    /// Estimate one token per two Unicode scalar values, rounding up.
    pub fn character_estimate() -> Self {
        Self {
            backend: Backend::CharacterEstimate,
        }
    }

    #[cfg(feature = "tiktoken")]
    pub fn cl100k_base() -> Self {
        Self {
            backend: Backend::Tiktoken(tiktoken_rs::cl100k_base_singleton()),
        }
    }

    #[cfg(feature = "tiktoken")]
    pub fn o200k_base() -> Self {
        Self {
            backend: Backend::Tiktoken(tiktoken_rs::o200k_base_singleton()),
        }
    }

    /// Load a Hugging Face tokenizer.json supplied by the caller.
    /// Padding and truncation are disabled so the full input is counted.
    #[cfg(feature = "huggingface")]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CountError> {
        let mut tokenizer = tokenizers::Tokenizer::from_bytes(bytes)
            .map_err(|error| CountError::InvalidTokenizer(error.to_string()))?;
        tokenizer.with_padding(None);
        tokenizer
            .with_truncation(None)
            .map_err(|error| CountError::InvalidTokenizer(error.to_string()))?;
        Ok(Self {
            backend: Backend::HuggingFace(Box::new(tokenizer)),
        })
    }

    /// Construct the bundled encoder. Retain this instance to avoid reparsing.
    #[cfg(feature = "bundled-deepseek")]
    pub fn deepseek_v4_pro() -> Result<Self, CountError> {
        Self::from_bytes(include_bytes!(
            "../assets/tokenizers/deepseek-v4-pro.tokenizer.json"
        ))
    }

    /// Count text without adding message framing or special tokens.
    pub fn count_text(&self, text: &str) -> Result<u64, CountError> {
        let count = match &self.backend {
            Backend::CharacterEstimate => text.chars().count().div_ceil(2),
            #[cfg(feature = "tiktoken")]
            Backend::Tiktoken(encoder) => encoder.encode_ordinary(text).len(),
            #[cfg(feature = "huggingface")]
            Backend::HuggingFace(encoder) => encoder
                .encode(text, false)
                .map_err(|error| CountError::Encode(error.to_string()))?
                .len(),
        };
        Ok(count as u64)
    }

    /// Count visible request content, joining fragments with newlines without
    /// adding message overhead.
    ///
    /// The caller validates the wire contract and supplies the final target
    /// request. This projection does not resolve references or count media.
    pub fn count_request(&self, format: RequestFormat, body: &[u8]) -> Result<u64, CountError> {
        let text = extract::extract(format, body)?;
        self.count_text(&text)
    }
}
