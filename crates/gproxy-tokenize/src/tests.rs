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
    use crate::{
        BoxFuture, ProgressReporter, RegistryError, TokenizerClient, TokenizerRegistry,
        TokenizerStore,
    };

    #[derive(Default)]
    struct Store(Mutex<BTreeMap<String, crate::StoredTokenizer>>);

    impl TokenizerStore for Store {
        fn list<'a>(&'a self) -> BoxFuture<'a, Result<Vec<String>, RegistryError>> {
            Box::pin(async move { Ok(self.0.lock().expect("store").keys().cloned().collect()) })
        }

        fn get<'a>(
            &'a self,
            name: &'a str,
        ) -> BoxFuture<'a, Result<Option<crate::StoredTokenizer>, RegistryError>> {
            Box::pin(async move { Ok(self.0.lock().expect("store").get(name).cloned()) })
        }

        fn put<'a>(
            &'a self,
            name: &'a str,
            repository: &'a str,
            bytes: &'a [u8],
        ) -> BoxFuture<'a, Result<(), RegistryError>> {
            Box::pin(async move {
                self.0.lock().expect("store").insert(
                    name.to_owned(),
                    crate::StoredTokenizer {
                        repository: repository.to_owned(),
                        bytes: bytes.to_vec(),
                    },
                );
                Ok(())
            })
        }
    }

    struct Offline;

    impl TokenizerClient for Offline {
        fn send<'a>(
            &'a self,
            _: http::Request<Bytes>,
            _: Option<ProgressReporter>,
        ) -> BoxFuture<'a, Result<http::Response<Bytes>, RegistryError>> {
            Box::pin(async { Err(RegistryError::new("offline")) })
        }
    }

    struct Available;

    impl TokenizerClient for Available {
        fn send<'a>(
            &'a self,
            _: http::Request<Bytes>,
            _: Option<ProgressReporter>,
        ) -> BoxFuture<'a, Result<http::Response<Bytes>, RegistryError>> {
            Box::pin(async {
                Ok(http::Response::new(Bytes::from_static(
                    crate::registry::bundled_bytes(),
                )))
            })
        }
    }

    struct Paused {
        started: Arc<tokio::sync::Notify>,
        resume: Arc<tokio::sync::Notify>,
    }

    #[derive(Default)]
    struct Redirecting(Mutex<u8>);

    impl TokenizerClient for Redirecting {
        fn send<'a>(
            &'a self,
            request: http::Request<Bytes>,
            _: Option<ProgressReporter>,
        ) -> BoxFuture<'a, Result<http::Response<Bytes>, RegistryError>> {
            Box::pin(async move {
                assert_eq!(
                    request.headers().get(http::header::ACCEPT_ENCODING),
                    Some(&http::HeaderValue::from_static("identity"))
                );
                let mut requests = self.0.lock().expect("requests");
                *requests += 1;
                if *requests == 1 {
                    assert_eq!(
                        request.uri().path(),
                        "/owner/model/resolve/main/tokenizer.json"
                    );
                    return Ok(http::Response::builder()
                        .status(http::StatusCode::TEMPORARY_REDIRECT)
                        .header(http::header::LOCATION, "/api/resolve-cache/tokenizer.json")
                        .body(Bytes::new())
                        .expect("redirect response"));
                }
                assert_eq!(request.uri().host(), Some("huggingface.co"));
                assert_eq!(request.uri().path(), "/api/resolve-cache/tokenizer.json");
                Ok(http::Response::new(Bytes::from_static(
                    crate::registry::bundled_bytes(),
                )))
            })
        }
    }

    impl TokenizerClient for Paused {
        fn send<'a>(
            &'a self,
            _: http::Request<Bytes>,
            progress: Option<ProgressReporter>,
        ) -> BoxFuture<'a, Result<http::Response<Bytes>, RegistryError>> {
            Box::pin(async move {
                let total = crate::registry::bundled_bytes().len() as u64;
                progress.expect("progress reporter")(crate::TokenizerDownloadProgress {
                    downloaded_bytes: total / 2,
                    total_bytes: Some(total),
                });
                self.started.notify_one();
                self.resume.notified().await;
                Ok(http::Response::new(Bytes::from_static(
                    crate::registry::bundled_bytes(),
                )))
            })
        }
    }

    #[tokio::test]
    async fn explicit_fetch_ignores_automatic_download_policy() {
        let store = Arc::new(Store::default());
        store.0.lock().expect("store").insert(
            "local-vocab".into(),
            crate::StoredTokenizer {
                repository: "owner/model".into(),
                bytes: b"invalid tokenizer".to_vec(),
            },
        );
        let registry = TokenizerRegistry::new(store.clone(), Arc::new(Available));

        registry
            .fetch("local-vocab", "owner/model")
            .await
            .expect("explicit fetch should download");

        assert!(registry.resolve("local-vocab").is_some());
        let stored = store
            .0
            .lock()
            .expect("store")
            .get("local-vocab")
            .cloned()
            .expect("stored alias");
        assert_eq!(stored.repository, "owner/model");
    }

    #[tokio::test]
    async fn explicit_fetch_reports_and_clears_download_progress() {
        let started = Arc::new(tokio::sync::Notify::new());
        let resume = Arc::new(tokio::sync::Notify::new());
        let registry = Arc::new(TokenizerRegistry::new(
            Arc::new(Store::default()),
            Arc::new(Paused {
                started: Arc::clone(&started),
                resume: Arc::clone(&resume),
            }),
        ));
        let fetch = {
            let registry = Arc::clone(&registry);
            tokio::spawn(async move { registry.fetch("local-vocab", "owner/model").await })
        };

        started.notified().await;
        let progress = registry
            .download_progress("local-vocab")
            .expect("active download progress");
        assert_eq!(progress.downloaded_bytes, progress.total_bytes.unwrap() / 2);

        resume.notify_one();
        fetch.await.expect("fetch task").expect("fetch tokenizer");
        assert_eq!(registry.download_progress("local-vocab"), None);
    }

    #[tokio::test]
    async fn explicit_fetch_follows_trusted_hugging_face_redirects() {
        let client = Arc::new(Redirecting::default());
        let registry = TokenizerRegistry::new(Arc::new(Store::default()), client.clone());

        registry
            .fetch("local-vocab", "owner/model")
            .await
            .expect("fetch tokenizer");

        assert_eq!(*client.0.lock().expect("requests"), 2);
    }

    #[tokio::test]
    async fn native_ladder_selects_exact_mapped_and_bundled_vocabularies() {
        let store = Arc::new(Store::default());
        store.0.lock().expect("store").insert(
            "shared-vocab".into(),
            crate::StoredTokenizer {
                repository: "owner/model".into(),
                bytes: crate::registry::bundled_bytes().to_vec(),
            },
        );
        let registry = TokenizerRegistry::new(store, Arc::new(Offline));
        registry
            .resolve_or_load("shared-vocab")
            .await
            .expect("hydrate mapped tokenizer");
        let mapped = json!({
            "claude-*": "shared-vocab",
            "kimi-*": "shared-vocab",
        });
        let cases = [
            ("gpt-4o-mini", None, CountMethod::Tiktoken),
            ("claude-sonnet-4", Some(&mapped), CountMethod::HuggingFace),
            ("kimi-k3", Some(&mapped), CountMethod::HuggingFace),
            ("qwen-max", None, CountMethod::BundledFallback),
        ];
        for (model, map, method) in cases {
            let result = count_detailed(model, &body(), map, &registry);
            assert_eq!(result.method, method, "{model}");
            assert!(result.tokens > 0, "{model}");
        }
    }
}
