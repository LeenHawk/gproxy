//! Local token counting (§6.3): tiktoken for gpt families, bundled/downloaded
//! HF tokenizers for the rest, char-estimate floor. Native-only behind
//! `count-local` except the estimate, which serves the edge build.

mod extract;
#[cfg(feature = "hf-registry")]
mod registry;

pub use extract::{harvest, try_harvest};
#[cfg(feature = "hf-registry")]
pub use registry::{
    LoadRequestStatus, TokenizerClient, TokenizerRegistry, TokenizerStore, VocabInfo, VocabSource,
};

/// What `count` receives as the registry: a real handle under `count-local`,
/// a unit on builds without it (edge) so call sites stay uniform.
#[cfg(feature = "hf-registry")]
pub type RegistryHandle<'a> = &'a TokenizerRegistry;
#[cfg(not(feature = "hf-registry"))]
pub type RegistryHandle<'a> = ();

/// Per-message fixed overhead (role/markup framing), in tokens.
const MSG_OVERHEAD: u64 = 4;

/// Count a single text buffer with the same local fallback Clove uses for
/// Claude Web usage synthesis: cl100k when local tokenizers are enabled,
/// otherwise the cross-target character estimate.
pub fn count_text(text: &str) -> u64 {
    #[cfg(feature = "tiktoken")]
    {
        tiktoken_rs::cl100k_base_singleton()
            .encode_ordinary(text)
            .len() as u64
    }
    #[cfg(not(feature = "tiktoken"))]
    {
        (text.chars().count() as u64).div_ceil(2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountMethod {
    Tiktoken,
    HuggingFace,
    BundledFallback,
    CharacterEstimate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CountWarning {
    ApproximateProviderFraming { tokens_per_message: u64 },
    GenericJsonHarvest,
    InvalidJson { reason: String },
    RawBodyEstimate,
    TokenizerLoadScheduled { vocab: String },
    TokenizerLoadInFlight { vocab: String },
    TokenizerNegativeCached { vocab: String },
    TokioRuntimeUnavailable { vocab: String },
    TokenizerEncodeFailed { vocab: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountResult {
    pub tokens: u64,
    pub method: CountMethod,
    pub vocab: Option<String>,
    pub warnings: Vec<CountWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CountError {
    InvalidJson(String),
    TokenizerUnavailable(String),
    TokenizerEncodeFailed(String),
    Registry(String),
}

impl std::fmt::Display for CountError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidJson(reason) => write!(formatter, "invalid request JSON: {reason}"),
            Self::TokenizerUnavailable(vocab) => {
                write!(formatter, "tokenizer `{vocab}` is unavailable")
            }
            Self::TokenizerEncodeFailed(vocab) => {
                write!(formatter, "tokenizer `{vocab}` failed to encode input")
            }
            Self::Registry(reason) => write!(formatter, "tokenizer registry failed: {reason}"),
        }
    }
}

impl std::error::Error for CountError {}

/// Count tokens of a provider-native request body. `map` = provider settings
/// `tokenizer_map` (glob → vocab name). Never fails: worst case is the
/// chars/2 estimate.
pub fn count(
    model: &str,
    body: &[u8],
    map: Option<&serde_json::Value>,
    registry: RegistryHandle,
) -> u64 {
    count_detailed(model, body, map, registry).tokens
}

/// Count with provenance and diagnostics. Unlike [`count`], malformed JSON is
/// never indistinguishable from a genuinely empty request: it falls back to a
/// conservative estimate over the raw body and records warnings.
pub fn count_detailed(
    model: &str,
    body: &[u8],
    map: Option<&serde_json::Value>,
    registry: RegistryHandle,
) -> CountResult {
    let (texts, messages, warnings) = match extract::try_harvest(body) {
        Ok((texts, messages)) => (
            texts,
            messages,
            vec![
                CountWarning::ApproximateProviderFraming {
                    tokens_per_message: MSG_OVERHEAD,
                },
                CountWarning::GenericJsonHarvest,
            ],
        ),
        Err(error) => (
            vec![String::from_utf8_lossy(body).into_owned()],
            0,
            vec![
                CountWarning::InvalidJson {
                    reason: error.to_string(),
                },
                CountWarning::RawBodyEstimate,
            ],
        ),
    };
    #[cfg(feature = "hf-registry")]
    let mut warnings = warnings;
    let overhead = messages * MSG_OVERHEAD;
    #[cfg(any(feature = "tiktoken", feature = "hf-registry"))]
    let joined = texts.join("\n");

    #[cfg(feature = "tiktoken")]
    {
        if let Some(bpe) = gpt_encoding(model) {
            let vocab = if O200K.iter().any(|prefix| model.starts_with(prefix)) {
                "o200k_base"
            } else {
                "cl100k_base"
            };
            return CountResult {
                tokens: bpe.encode_ordinary(&joined).len() as u64 + overhead,
                method: CountMethod::Tiktoken,
                vocab: Some(vocab.to_owned()),
                warnings,
            };
        }
    }
    #[cfg(feature = "hf-registry")]
    {
        // tokenizer_map glob hit → that vocab; otherwise resolve the model
        // name itself. Miss → request a background hydrate/download and fall
        // through.
        let name = select_vocab(map, model).unwrap_or_else(|| model.to_owned());
        if let Some(tok) = registry.resolve(&name) {
            if let Some(n) = encode_len(&tok, &joined) {
                return CountResult {
                    tokens: n + overhead,
                    method: if matches!(name.as_str(), "deepseek" | "deepseek-v4-pro") {
                        CountMethod::BundledFallback
                    } else {
                        CountMethod::HuggingFace
                    },
                    vocab: Some(name),
                    warnings,
                };
            }
            warnings.push(CountWarning::TokenizerEncodeFailed { vocab: name });
        } else {
            let warning = match registry.request_load(&name) {
                LoadRequestStatus::Scheduled => CountWarning::TokenizerLoadScheduled {
                    vocab: name.clone(),
                },
                LoadRequestStatus::AlreadyInFlight => CountWarning::TokenizerLoadInFlight {
                    vocab: name.clone(),
                },
                LoadRequestStatus::NegativeCached => CountWarning::TokenizerNegativeCached {
                    vocab: name.clone(),
                },
                LoadRequestStatus::NoRuntime => CountWarning::TokioRuntimeUnavailable {
                    vocab: name.clone(),
                },
            };
            warnings.push(warning);
        }
        // Bundled fallback vocab.
        #[cfg(feature = "bundled-fallback")]
        if let Some(tok) = registry.resolve("deepseek")
            && let Some(n) = encode_len(&tok, &joined)
        {
            return CountResult {
                tokens: n + overhead,
                method: CountMethod::BundledFallback,
                vocab: Some("deepseek-v4-pro".to_owned()),
                warnings,
            };
        }
    }
    #[cfg(not(feature = "hf-registry"))]
    let _ = (model, map, registry);

    let chars: usize = texts.iter().map(|t| t.chars().count()).sum();
    CountResult {
        tokens: (chars as u64).div_ceil(2) + overhead,
        method: CountMethod::CharacterEstimate,
        vocab: None,
        warnings,
    }
}

/// Strict counting for correctness-sensitive paths. This rejects malformed
/// JSON and, with `hf-registry`, waits for store hydration/download instead of
/// returning a result that changes after a background load.
pub async fn try_count(
    model: &str,
    body: &[u8],
    map: Option<&serde_json::Value>,
    registry: RegistryHandle<'_>,
) -> Result<CountResult, CountError> {
    let (texts, messages) =
        extract::try_harvest(body).map_err(|error| CountError::InvalidJson(error.to_string()))?;
    let joined = texts.join("\n");
    let overhead = messages * MSG_OVERHEAD;
    let warnings = vec![
        CountWarning::ApproximateProviderFraming {
            tokens_per_message: MSG_OVERHEAD,
        },
        CountWarning::GenericJsonHarvest,
    ];

    #[cfg(feature = "tiktoken")]
    if let Some(bpe) = gpt_encoding(model) {
        let vocab = if O200K.iter().any(|prefix| model.starts_with(prefix)) {
            "o200k_base"
        } else {
            "cl100k_base"
        };
        return Ok(CountResult {
            tokens: bpe.encode_ordinary(&joined).len() as u64 + overhead,
            method: CountMethod::Tiktoken,
            vocab: Some(vocab.to_owned()),
            warnings,
        });
    }

    #[cfg(feature = "hf-registry")]
    {
        let name = select_vocab(map, model).unwrap_or_else(|| model.to_owned());
        let tokenizer = match registry.resolve(&name) {
            Some(tokenizer) => Some(tokenizer),
            None => registry
                .resolve_or_load(&name)
                .await
                .map_err(|error| CountError::Registry(error.to_string()))?,
        }
        .ok_or_else(|| CountError::TokenizerUnavailable(name.clone()))?;
        let tokens = encode_len(&tokenizer, &joined)
            .ok_or_else(|| CountError::TokenizerEncodeFailed(name.clone()))?
            + overhead;
        Ok(CountResult {
            tokens,
            method: if matches!(name.as_str(), "deepseek" | "deepseek-v4-pro") {
                CountMethod::BundledFallback
            } else {
                CountMethod::HuggingFace
            },
            vocab: Some(name),
            warnings,
        })
    }

    #[cfg(not(feature = "hf-registry"))]
    {
        let _ = (joined, overhead, warnings, map, registry);
        Err(CountError::TokenizerUnavailable(model.to_owned()))
    }
}

/// `*`-wildcard glob matching, anchored at both ends. No other metacharacters.
#[cfg(feature = "hf-registry")]
fn glob_matches(pattern: &str, value: &str) -> bool {
    let (pattern, value) = (pattern.as_bytes(), value.as_bytes());
    let (mut p, mut v, mut star, mut retry_v) = (0, 0, None, 0);
    while v < value.len() {
        if p < pattern.len() && pattern[p] == value[v] {
            p += 1;
            v += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            retry_v = v;
        } else if let Some(star_index) = star {
            p = star_index + 1;
            retry_v += 1;
            v = retry_v;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

/// Legacy JSON-object maps use deterministic "most specific pattern wins"
/// semantics; ties are resolved by lexical pattern order. Callers no longer
/// depend on `serde_json::Map`'s storage ordering.
#[cfg(feature = "hf-registry")]
fn select_vocab(map: Option<&serde_json::Value>, model: &str) -> Option<String> {
    let mut best: Option<(&str, usize, &str)> = None;
    for (pattern, value) in map?.as_object()? {
        let Some(vocab) = value.as_str() else {
            continue;
        };
        if !glob_matches(pattern, model) {
            continue;
        }
        let specificity = pattern.bytes().filter(|byte| *byte != b'*').count();
        if best.is_none_or(|(best_pattern, best_specificity, _)| {
            specificity > best_specificity
                || (specificity == best_specificity && pattern.as_str() < best_pattern)
        }) {
            best = Some((pattern, specificity, vocab));
        }
    }
    best.map(|(_, _, vocab)| vocab.to_owned())
}

/// gpt-family prefixes with a tiktoken builtin (o200k / cl100k).
const O200K: &[&str] = &["gpt-4o", "gpt-4.1", "gpt-5", "o1", "o3", "o4"];
const CL100K: &[&str] = &["gpt-3.5", "gpt-4"];

/// Whether `model` belongs to a gpt family with an exact local tiktoken
/// vocabulary (drives the §17 counting-ladder source label).
pub fn is_gpt_family(model: &str) -> bool {
    O200K.iter().chain(CL100K).any(|p| model.starts_with(p))
}

/// tiktoken builtin for gpt families; `None` = not a gpt model.
#[cfg(feature = "tiktoken")]
fn gpt_encoding(model: &str) -> Option<&'static tiktoken_rs::CoreBPE> {
    if O200K.iter().any(|p| model.starts_with(p)) {
        Some(tiktoken_rs::o200k_base_singleton())
    } else if CL100K.iter().any(|p| model.starts_with(p)) {
        Some(tiktoken_rs::cl100k_base_singleton())
    } else {
        None
    }
}

#[cfg(feature = "hf-registry")]
fn encode_len(tok: &tokenizers::Tokenizer, text: &str) -> Option<u64> {
    Some(tok.encode(text, false).ok()?.get_ids().len() as u64)
}

#[cfg(test)]
mod general_tests {
    use super::*;

    #[test]
    fn malformed_json_uses_raw_body_estimate_with_diagnostics() {
        let body = br#"{"messages":[{"content":"important text"}"#;
        #[cfg(feature = "hf-registry")]
        let registry = test_registry();
        #[cfg(feature = "hf-registry")]
        let result = count_detailed("unknown", body, None, &registry);
        #[cfg(not(feature = "hf-registry"))]
        let result = count_detailed("unknown", body, None, ());
        assert!(result.tokens > 0);
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| matches!(warning, CountWarning::InvalidJson { .. }))
        );
        assert!(result.warnings.contains(&CountWarning::RawBodyEstimate));
    }

    #[cfg(feature = "hf-registry")]
    fn test_registry() -> TokenizerRegistry {
        use std::sync::Arc;

        struct Store;
        #[async_trait::async_trait]
        impl TokenizerStore for Store {
            async fn list_tokenizer_vocabs(&self) -> anyhow::Result<Vec<String>> {
                Ok(Vec::new())
            }
            async fn get_tokenizer_vocab(&self, _: &str) -> anyhow::Result<Option<Vec<u8>>> {
                Ok(None)
            }
            async fn put_tokenizer_vocab(&self, _: &str, _: &[u8]) -> anyhow::Result<()> {
                Ok(())
            }
        }
        struct Client;
        #[async_trait::async_trait]
        impl TokenizerClient for Client {
            async fn send(
                &self,
                _: http::Request<bytes::Bytes>,
            ) -> anyhow::Result<http::Response<bytes::Bytes>> {
                anyhow::bail!("not used")
            }
        }
        TokenizerRegistry::new(Arc::new(Store), Arc::new(Client))
    }

    #[cfg(feature = "hf-registry")]
    #[test]
    fn glob_selection_is_specific_and_stable() {
        let map = serde_json::json!({
            "*": "generic",
            "claude-*": "claude",
            "claude-3-*": "claude-3"
        });
        assert_eq!(
            select_vocab(Some(&map), "claude-3-opus").as_deref(),
            Some("claude-3")
        );
        assert_eq!(
            select_vocab(Some(&map), "claude-next").as_deref(),
            Some("claude")
        );
    }

    #[cfg(feature = "hf-registry")]
    #[test]
    fn background_load_without_runtime_is_explicit() {
        let registry = test_registry();
        assert_eq!(
            registry.request_load("owner/model"),
            LoadRequestStatus::NoRuntime
        );
    }
}

#[cfg(all(test, feature = "count-local"))]
mod tests {
    use std::sync::Arc;

    use bytes::Bytes;

    use super::{
        CountMethod, TokenizerClient, TokenizerRegistry, TokenizerStore, count, count_detailed,
    };

    /// No-op upstream: the registry never dials out in these tests.
    struct NoUpstream;

    #[async_trait::async_trait]
    impl TokenizerClient for NoUpstream {
        async fn send(&self, _req: http::Request<Bytes>) -> anyhow::Result<http::Response<Bytes>> {
            anyhow::bail!("no upstream in tests")
        }
    }

    #[derive(Default)]
    struct EmptyStore;

    #[async_trait::async_trait]
    impl TokenizerStore for EmptyStore {
        async fn list_tokenizer_vocabs(&self) -> anyhow::Result<Vec<String>> {
            Ok(Vec::new())
        }

        async fn get_tokenizer_vocab(&self, _name: &str) -> anyhow::Result<Option<Vec<u8>>> {
            Ok(None)
        }

        async fn put_tokenizer_vocab(&self, _name: &str, _bytes: &[u8]) -> anyhow::Result<()> {
            Ok(())
        }
    }

    async fn registry() -> TokenizerRegistry {
        TokenizerRegistry::new(Arc::new(EmptyStore), Arc::new(NoUpstream))
    }

    fn chat_body() -> Vec<u8> {
        serde_json::json!({
            "model": "x",
            "messages": [{ "role": "user", "content": "Hello, how are you today?" }]
        })
        .to_string()
        .into_bytes()
    }

    #[tokio::test]
    async fn tiktoken_gpt_path_is_stable() {
        let reg = registry().await;
        let a = count("gpt-4o-mini", &chat_body(), None, &reg);
        let b = count("gpt-4o-mini", &chat_body(), None, &reg);
        assert!(a > 0);
        assert_eq!(a, b);
        let result = count_detailed("gpt-4o-mini", &chat_body(), None, &reg);
        assert_eq!(result.method, CountMethod::Tiktoken);
        assert_eq!(result.vocab.as_deref(), Some("o200k_base"));
    }

    #[tokio::test]
    async fn bundled_deepseek_covers_unknown_models() {
        let reg = registry().await;
        assert!(reg.resolve("deepseek").is_some());
        assert!(count("qwen-max", &chat_body(), None, &reg) > 0);
        let result = count_detailed("qwen-max", &chat_body(), None, &reg);
        assert_eq!(result.method, CountMethod::BundledFallback);
    }
}
