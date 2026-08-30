#[cfg(any(feature = "tiktoken", feature = "hf-registry"))]
use crate::CountMethod;
use crate::count::{MESSAGE_OVERHEAD, Prepared, RegistryHandle};
use crate::{CountError, CountResult};

pub async fn try_count(
    model: &str,
    body: &[u8],
    map: Option<&serde_json::Value>,
    registry: RegistryHandle<'_>,
) -> Result<CountResult, CountError> {
    let (texts, messages) =
        crate::try_harvest(body).map_err(|error| CountError::InvalidJson(error.to_string()))?;
    let prepared = Prepared::from_harvest(texts, messages);
    let overhead = prepared.messages.saturating_mul(MESSAGE_OVERHEAD);

    #[cfg(feature = "tiktoken")]
    if let Some((encoding, vocab)) = crate::model::gpt_encoding(model) {
        return Ok(CountResult {
            tokens: encoding.encode_ordinary(&prepared.joined).len() as u64 + overhead,
            method: CountMethod::Tiktoken,
            vocab: Some(vocab.to_owned()),
            warnings: prepared.warnings,
        });
    }

    #[cfg(feature = "hf-registry")]
    {
        if !registry.vocabs_enabled() {
            return Ok(prepared.character_result());
        }
        let name = crate::model::select_vocab(map, model)
            .or_else(|| registry.default_vocab())
            .unwrap_or_else(|| model.to_owned());
        let tokenizer = match registry.resolve(&name) {
            Some(tokenizer) => Some(tokenizer),
            None => registry
                .resolve_or_load(&name)
                .await
                .map_err(|error| CountError::Registry(error.to_string()))?,
        }
        .ok_or_else(|| CountError::TokenizerUnavailable(name.clone()))?;
        let tokens = tokenizer
            .encode(prepared.joined.as_str(), false)
            .map_err(|_| CountError::TokenizerEncodeFailed(name.clone()))?
            .get_ids()
            .len() as u64;
        Ok(CountResult {
            tokens: tokens + overhead,
            method: if crate::registry::BUNDLED_NAMES.contains(&name.as_str()) {
                CountMethod::BundledFallback
            } else {
                CountMethod::HuggingFace
            },
            vocab: Some(name),
            warnings: prepared.warnings,
        })
    }

    #[cfg(not(feature = "hf-registry"))]
    {
        let _ = (map, registry, overhead, prepared);
        Err(CountError::TokenizerUnavailable(model.to_owned()))
    }
}
