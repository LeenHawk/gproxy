use std::sync::{Arc, OnceLock};

use tokenizers::Tokenizer;

pub(crate) const BUNDLED_PRIMARY: &str = "deepseek";
pub(crate) const BUNDLED_NAMES: &[&str] = &[BUNDLED_PRIMARY, "deepseek-v4-pro"];

static BYTES: &[u8] = include_bytes!("../../assets/tokenizers/deepseek-v4-pro.tokenizer.json");
static TOKENIZER: OnceLock<Option<Arc<Tokenizer>>> = OnceLock::new();

pub(super) fn tokenizer() -> Option<Arc<Tokenizer>> {
    TOKENIZER
        .get_or_init(|| match Tokenizer::from_bytes(BYTES) {
            Ok(tokenizer) => Some(Arc::new(tokenizer)),
            Err(error) => {
                tracing::error!(error = %error, "bundled tokenizer failed to parse");
                None
            }
        })
        .clone()
}

#[cfg(test)]
pub(crate) fn bytes() -> &'static [u8] {
    BYTES
}
