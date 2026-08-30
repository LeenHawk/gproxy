#[cfg(any(feature = "tiktoken", feature = "hf-registry"))]
use crate::CountMethod;
use crate::CountResult;
#[cfg(feature = "hf-registry")]
use crate::CountWarning;
use crate::count::{MESSAGE_OVERHEAD, Prepared, RegistryHandle};

pub fn count(
    model: &str,
    body: &[u8],
    map: Option<&serde_json::Value>,
    registry: RegistryHandle<'_>,
) -> u64 {
    count_detailed(model, body, map, registry).tokens
}

pub fn count_detailed(
    model: &str,
    body: &[u8],
    map: Option<&serde_json::Value>,
    registry: RegistryHandle<'_>,
) -> CountResult {
    #[cfg(feature = "hf-registry")]
    let mut prepared = Prepared::lossy(body);
    #[cfg(not(feature = "hf-registry"))]
    let prepared = Prepared::lossy(body);
    let overhead = prepared.messages.saturating_mul(MESSAGE_OVERHEAD);

    #[cfg(feature = "tiktoken")]
    if let Some((encoding, vocab)) = crate::model::gpt_encoding(model) {
        return CountResult {
            tokens: encoding.encode_ordinary(&prepared.joined).len() as u64 + overhead,
            method: CountMethod::Tiktoken,
            vocab: Some(vocab.to_owned()),
            warnings: prepared.warnings,
        };
    }

    #[cfg(feature = "hf-registry")]
    if registry.vocabs_enabled() {
        let name = crate::model::select_vocab(map, model)
            .or_else(|| registry.default_vocab())
            .unwrap_or_else(|| model.to_owned());
        if let Some(tokenizer) = registry.resolve(&name) {
            if let Some(tokens) = encode_len(&tokenizer, &prepared.joined) {
                return result(tokens + overhead, &name, prepared.warnings);
            }
            prepared
                .warnings
                .push(CountWarning::TokenizerEncodeFailed { vocab: name });
        } else {
            let warning = match registry.request_load(&name) {
                crate::LoadRequestStatus::Scheduled => CountWarning::TokenizerLoadScheduled {
                    vocab: name.clone(),
                },
                crate::LoadRequestStatus::AlreadyInFlight => CountWarning::TokenizerLoadInFlight {
                    vocab: name.clone(),
                },
                crate::LoadRequestStatus::NegativeCached => CountWarning::TokenizerNegativeCached {
                    vocab: name.clone(),
                },
                crate::LoadRequestStatus::NoRuntime => CountWarning::TokioRuntimeUnavailable {
                    vocab: name.clone(),
                },
            };
            prepared.warnings.push(warning);
        }

        #[cfg(feature = "bundled-fallback")]
        if let Some(tokenizer) = registry.resolve(crate::registry::BUNDLED_PRIMARY)
            && let Some(tokens) = encode_len(&tokenizer, &prepared.joined)
        {
            return CountResult {
                tokens: tokens + overhead,
                method: CountMethod::BundledFallback,
                vocab: Some("deepseek-v4-pro".to_owned()),
                warnings: prepared.warnings,
            };
        }
    }
    #[cfg(not(feature = "hf-registry"))]
    let _ = (model, map, registry, overhead);

    prepared.character_result()
}

#[cfg(feature = "hf-registry")]
fn encode_len(tokenizer: &tokenizers::Tokenizer, text: &str) -> Option<u64> {
    Some(tokenizer.encode(text, false).ok()?.get_ids().len() as u64)
}

#[cfg(feature = "hf-registry")]
fn result(tokens: u64, name: &str, warnings: Vec<CountWarning>) -> CountResult {
    CountResult {
        tokens,
        method: if crate::registry::BUNDLED_NAMES.contains(&name) {
            CountMethod::BundledFallback
        } else {
            CountMethod::HuggingFace
        },
        vocab: Some(name.to_owned()),
        warnings,
    }
}
