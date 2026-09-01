use serde_json::json;

use crate::{CountMethod, count_detailed, try_harvest};

fn body() -> Vec<u8> {
    json!({
        "model": "model",
        "system": "be terse",
        "messages": [{"role": "user", "content": "Hello, world"}],
        "tools": [{"name": "lookup", "description": "look something up"}]
    })
    .to_string()
    .into_bytes()
}

#[test]
fn harvests_provider_text_and_framing() {
    let (texts, messages) = try_harvest(&body()).expect("valid request");
    assert_eq!(messages, 1);
    for expected in ["be terse", "Hello, world", "lookup"] {
        assert!(texts.iter().any(|text| text.contains(expected)));
    }
}

#[cfg(not(feature = "hf-registry"))]
#[test]
fn unavailable_local_tokenizer_uses_character_rung() {
    let result = count_detailed("unknown", &body(), None, ());
    assert_eq!(result.method, CountMethod::CharacterEstimate);
    assert!(result.tokens > 0);
}

#[cfg(feature = "count-local")]
mod local {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use bytes::Bytes;

    use super::*;
    use crate::{BoxFuture, RegistryError, TokenizerClient, TokenizerRegistry, TokenizerStore};

    #[derive(Default)]
    struct Store(Mutex<BTreeMap<String, Vec<u8>>>);

    impl TokenizerStore for Store {
        fn list<'a>(&'a self) -> BoxFuture<'a, Result<Vec<String>, RegistryError>> {
            Box::pin(async move { Ok(self.0.lock().expect("store").keys().cloned().collect()) })
        }

        fn get<'a>(
            &'a self,
            name: &'a str,
        ) -> BoxFuture<'a, Result<Option<Vec<u8>>, RegistryError>> {
            Box::pin(async move { Ok(self.0.lock().expect("store").get(name).cloned()) })
        }

        fn put<'a>(
            &'a self,
            name: &'a str,
            bytes: &'a [u8],
        ) -> BoxFuture<'a, Result<(), RegistryError>> {
            Box::pin(async move {
                self.0
                    .lock()
                    .expect("store")
                    .insert(name.to_owned(), bytes.to_vec());
                Ok(())
            })
        }
    }

    struct Offline;

    impl TokenizerClient for Offline {
        fn send<'a>(
            &'a self,
            _: http::Request<Bytes>,
        ) -> BoxFuture<'a, Result<http::Response<Bytes>, RegistryError>> {
            Box::pin(async { Err(RegistryError::new("offline")) })
        }
    }

    struct Available;

    impl TokenizerClient for Available {
        fn send<'a>(
            &'a self,
            _: http::Request<Bytes>,
        ) -> BoxFuture<'a, Result<http::Response<Bytes>, RegistryError>> {
            Box::pin(async {
                Ok(http::Response::new(Bytes::from_static(
                    crate::registry::bundled_bytes(),
                )))
            })
        }
    }

    #[tokio::test]
    async fn explicit_fetch_ignores_automatic_download_policy() {
        let store = Arc::new(Store::default());
        let registry = TokenizerRegistry::new(store.clone(), Arc::new(Available));

        registry
            .fetch("owner/model")
            .await
            .expect("explicit fetch should download");

        assert!(registry.resolve("owner/model").is_some());
        assert!(store.0.lock().expect("store").contains_key("owner/model"));
    }

    #[tokio::test]
    async fn native_ladder_selects_exact_mapped_and_bundled_vocabularies() {
        let store = Arc::new(Store::default());
        store.0.lock().expect("store").insert(
            "owner/model".into(),
            crate::registry::bundled_bytes().to_vec(),
        );
        let registry = TokenizerRegistry::new(store, Arc::new(Offline));
        registry
            .resolve_or_load("owner/model")
            .await
            .expect("hydrate mapped tokenizer");
        let mapped = json!({"claude-*": "owner/model"});
        let cases = [
            ("gpt-4o-mini", None, CountMethod::Tiktoken),
            ("claude-sonnet-4", Some(&mapped), CountMethod::HuggingFace),
            ("qwen-max", None, CountMethod::BundledFallback),
        ];
        for (model, map, method) in cases {
            let result = count_detailed(model, &body(), map, &registry);
            assert_eq!(result.method, method, "{model}");
            assert!(result.tokens > 0, "{model}");
        }
    }
}
