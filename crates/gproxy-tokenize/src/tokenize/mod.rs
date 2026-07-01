//! Local token counting (§6.3): tiktoken for gpt families, bundled/downloaded
//! HF tokenizers for the rest, char-estimate floor. Native-only behind
//! `count-local` except the estimate, which serves the edge build.

mod extract;
#[cfg(feature = "count-local")]
mod registry;

pub use extract::harvest;
#[cfg(feature = "count-local")]
pub use registry::{TokenizerClient, TokenizerRegistry, TokenizerStore, VocabInfo, VocabSource};

/// What `count` receives as the registry: a real handle under `count-local`,
/// a unit on builds without it (edge) so call sites stay uniform.
#[cfg(feature = "count-local")]
pub type RegistryHandle<'a> = &'a TokenizerRegistry;
#[cfg(not(feature = "count-local"))]
pub type RegistryHandle<'a> = ();

/// Per-message fixed overhead (role/markup framing), in tokens.
const MSG_OVERHEAD: u64 = 4;

/// Count tokens of a provider-native request body. `map` = provider settings
/// `tokenizer_map` (glob → vocab name). Never fails: worst case is the
/// chars/2 estimate.
pub fn count(
    model: &str,
    body: &[u8],
    map: Option<&serde_json::Value>,
    registry: RegistryHandle,
) -> u64 {
    let (texts, messages) = extract::harvest(body);
    let overhead = messages * MSG_OVERHEAD;

    #[cfg(feature = "count-local")]
    {
        let joined = texts.join("\n");
        if let Some(bpe) = gpt_encoding(model) {
            return bpe.encode_ordinary(&joined).len() as u64 + overhead;
        }
        // tokenizer_map glob hit → that vocab; otherwise resolve the model
        // name itself. Miss → request a background hydrate/download and fall
        // through.
        let name = map
            .and_then(|m| m.as_object())
            .and_then(|obj| {
                obj.iter()
                    .find(|(pat, _)| glob_matches(pat, model))
                    .and_then(|(_, v)| v.as_str().map(str::to_owned))
            })
            .unwrap_or_else(|| model.to_owned());
        if let Some(tok) = registry.resolve(&name) {
            if let Some(n) = encode_len(&tok, &joined) {
                return n + overhead;
            }
        } else {
            registry.request_load(&name);
        }
        // Bundled fallback vocab.
        if let Some(tok) = registry.resolve("deepseek")
            && let Some(n) = encode_len(&tok, &joined)
        {
            return n + overhead;
        }
    }
    #[cfg(not(feature = "count-local"))]
    let _ = (model, map, registry);

    let chars: usize = texts.iter().map(|t| t.chars().count()).sum();
    (chars as u64).div_ceil(2) + overhead
}

/// `*`-wildcard glob matching, anchored at both ends. No other metacharacters.
#[cfg(feature = "count-local")]
fn glob_matches(pattern: &str, value: &str) -> bool {
    fn inner(p: &[u8], v: &[u8]) -> bool {
        match p.split_first() {
            None => v.is_empty(),
            Some((b'*', rest)) => (0..=v.len()).any(|i| inner(rest, &v[i..])),
            Some((c, rest)) => v
                .split_first()
                .is_some_and(|(vc, vrest)| vc == c && inner(rest, vrest)),
        }
    }
    inner(pattern.as_bytes(), value.as_bytes())
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
#[cfg(feature = "count-local")]
fn gpt_encoding(model: &str) -> Option<&'static tiktoken_rs::CoreBPE> {
    if O200K.iter().any(|p| model.starts_with(p)) {
        Some(tiktoken_rs::o200k_base_singleton())
    } else if CL100K.iter().any(|p| model.starts_with(p)) {
        Some(tiktoken_rs::cl100k_base_singleton())
    } else {
        None
    }
}

#[cfg(feature = "count-local")]
fn encode_len(tok: &tokenizers::Tokenizer, text: &str) -> Option<u64> {
    Some(tok.encode(text, false).ok()?.get_ids().len() as u64)
}

#[cfg(all(test, feature = "count-local"))]
mod tests {
    use std::sync::Arc;

    use bytes::Bytes;

    use super::{TokenizerClient, TokenizerRegistry, TokenizerStore, count};

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
    }

    #[tokio::test]
    async fn bundled_deepseek_covers_unknown_models() {
        let reg = registry().await;
        assert!(reg.resolve("deepseek").is_some());
        assert!(count("qwen-max", &chat_body(), None, &reg) > 0);
    }
}
