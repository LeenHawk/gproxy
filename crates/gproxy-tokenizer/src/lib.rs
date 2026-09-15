//! Local token counting, with no network, persistence or task runtime.
//!
//! Callers select and retain a tokenizer, pass a string, and receive its token
//! count. No request parsing, message overhead or chat template is applied.

#![doc = include_str!("../README.md")]

mod error;
#[cfg(feature = "local")]
mod model;
#[cfg(feature = "huggingface")]
mod vocabulary;

pub use error::CountError;
#[cfg(feature = "huggingface")]
pub use vocabulary::Vocabulary;

/// A reusable encoder. Cloning shares the vocabulary rather than copying it.
#[derive(Clone)]
pub struct Tokenizer {
    backend: Backend,
}

#[derive(Clone)]
enum Backend {
    CharacterEstimate,
    #[cfg(feature = "tiktoken")]
    Tiktoken(&'static tiktoken_rs::CoreBPE),
    #[cfg(feature = "huggingface")]
    HuggingFace(std::sync::Arc<tokenizers::Tokenizer>),
}

impl Tokenizer {
    /// GPT family: tiktoken. Otherwise: supplied vocabulary, then bundled DeepSeek V4.
    /// The supplied vocabulary is shared, never reparsed or deep-cloned here.
    #[cfg(feature = "local")]
    pub fn for_model(model: &str, vocabulary: Option<&Vocabulary>) -> Result<Self, CountError> {
        if let Some(encoder) = model::gpt_encoding(model) {
            return Ok(Self {
                backend: Backend::Tiktoken(encoder),
            });
        }
        match vocabulary {
            Some(vocabulary) => Ok(Self::from_vocabulary(vocabulary)),
            None => Self::deepseek_v4_pro(),
        }
    }

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

    /// Share a vocabulary already loaded by the caller.
    #[cfg(feature = "huggingface")]
    pub fn from_vocabulary(vocabulary: &Vocabulary) -> Self {
        Self {
            backend: Backend::HuggingFace(std::sync::Arc::clone(&vocabulary.0)),
        }
    }

    /// Share the bundled encoder, parsed once on first use.
    #[cfg(feature = "bundled-deepseek")]
    pub fn deepseek_v4_pro() -> Result<Self, CountError> {
        vocabulary::bundled().map(Self::from_vocabulary)
    }

    /// Count text without adding message framing or special tokens.
    pub fn count(&self, text: &str) -> Result<u64, CountError> {
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
}
