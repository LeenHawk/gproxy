use std::sync::Arc;

use crate::CountError;

/// A parsed Hugging Face vocabulary. Cloning only increments a reference count.
#[derive(Clone)]
pub struct Vocabulary(pub(crate) Arc<tokenizers::Tokenizer>);

impl Vocabulary {
    /// Parse caller-supplied tokenizer.json bytes once, without copying the bytes.
    /// Disable padding and truncation so counting covers the full input.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CountError> {
        let mut tokenizer = tokenizers::Tokenizer::from_bytes(bytes)
            .map_err(|error| CountError::InvalidTokenizer(error.to_string()))?;
        tokenizer.with_padding(None);
        tokenizer
            .with_truncation(None)
            .map_err(|error| CountError::InvalidTokenizer(error.to_string()))?;
        Ok(Self(Arc::new(tokenizer)))
    }
}

#[cfg(feature = "bundled-deepseek")]
pub(crate) fn bundled() -> Result<&'static Vocabulary, CountError> {
    static VOCABULARY: std::sync::OnceLock<Result<Vocabulary, CountError>> =
        std::sync::OnceLock::new();
    VOCABULARY
        .get_or_init(|| {
            Vocabulary::from_bytes(include_bytes!(
                "../assets/tokenizers/deepseek-v4-pro.tokenizer.json"
            ))
        })
        .as_ref()
        .map_err(Clone::clone)
}
