use super::*;

use crate::protocol::Provider;

#[test]
fn parse_openai_and_gemini() {
    let oa = br#"{"object":"list","data":[{"id":"gpt-4o","context_length":128000,"max_completion_tokens":16384,"supported_parameters":["reasoning","tools"]},{"id":"llama-local","meta":{"n_ctx":95232,"n_ctx_train":262144},"supported_parameters":[]},{"id":"gpt-4o-mini"}]}"#;
    let ids: Vec<_> = parse_models(Provider::OpenAi, oa)
        .into_iter()
        .map(|m| m.id)
        .collect();
    assert_eq!(ids, ["gpt-4o", "llama-local", "gpt-4o-mini"]);
    let oa = parse_models(Provider::OpenAi, oa);
    assert_eq!(oa[0].context_window, Some(128_000));
    assert_eq!(oa[0].max_output_tokens, Some(16_384));
    assert_eq!(oa[1].context_window, Some(95_232));
    assert_eq!(oa[0].thinking_supported, Some(true));
    assert_eq!(oa[1].thinking_supported, Some(false));
    assert_eq!(oa[2].thinking_supported, None);

    let gm = br#"{"models":[{"name":"models/gemini-1.5-pro","displayName":"Gemini 1.5 Pro","inputTokenLimit":1048576,"outputTokenLimit":8192,"thinking":true}]}"#;
    let g = parse_models(Provider::Gemini, gm);
    assert_eq!(g[0].id, "gemini-1.5-pro");
    assert_eq!(g[0].display_name.as_deref(), Some("Gemini 1.5 Pro"));
    assert_eq!(g[0].max_input_tokens, Some(1_048_576));
    assert_eq!(g[0].max_output_tokens, Some(8_192));
    assert_eq!(g[0].thinking_supported, Some(true));

    let cl = br#"{"data":[{"id":"claude-test","display_name":"Claude Test","max_input_tokens":200000,"max_tokens":32000,"capabilities":{"thinking":{"supported":true,"types":{"adaptive":{"supported":true},"enabled":{"supported":false}}}}}]}"#;
    let c = parse_models(Provider::Claude, cl);
    assert_eq!(c[0].thinking_supported, Some(true));
    assert_eq!(c[0].thinking_adaptive_supported, Some(true));
    assert_eq!(c[0].thinking_enabled_supported, Some(false));
}

#[test]
fn parse_openai_provider_enrichment_fields() {
    let models = parse_models(
        Provider::OpenAi,
        br#"{"data":[{"id":"grok-4.6","display_name":"Grok 4.6","context_length":500000,"thinking_supported":true,"thinking_adaptive_supported":false,"thinking_enabled_supported":true}]}"#,
    );
    assert_eq!(models[0].display_name.as_deref(), Some("Grok 4.6"));
    assert_eq!(models[0].context_window, Some(500_000));
    assert_eq!(models[0].thinking_supported, Some(true));
    assert_eq!(models[0].thinking_adaptive_supported, Some(false));
    assert_eq!(models[0].thinking_enabled_supported, Some(true));
}

#[test]
fn merge_models_keeps_first_order_and_fills_missing_display_name() {
    let mut models = vec![UpstreamModel {
        id: "shared".into(),
        display_name: None,
        context_window: None,
        max_input_tokens: None,
        max_output_tokens: None,
        thinking_supported: None,
        thinking_adaptive_supported: None,
        thinking_enabled_supported: None,
    }];
    let mut indexes = std::collections::HashMap::from([("shared".to_string(), 0)]);

    merge_models(
        &mut models,
        &mut indexes,
        vec![
            UpstreamModel {
                id: "shared".into(),
                display_name: Some("Shared model".into()),
                context_window: Some(100_000),
                max_input_tokens: None,
                max_output_tokens: Some(8_000),
                thinking_supported: Some(true),
                thinking_adaptive_supported: Some(false),
                thinking_enabled_supported: Some(true),
            },
            UpstreamModel {
                id: "new".into(),
                display_name: Some("New model".into()),
                context_window: None,
                max_input_tokens: Some(64_000),
                max_output_tokens: None,
                thinking_supported: None,
                thinking_adaptive_supported: None,
                thinking_enabled_supported: None,
            },
        ],
    );

    assert_eq!(
        models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        ["shared", "new"]
    );
    assert_eq!(models[0].display_name.as_deref(), Some("Shared model"));
    assert_eq!(models[0].context_window, Some(100_000));
    assert_eq!(models[0].max_output_tokens, Some(8_000));
    assert_eq!(models[0].thinking_supported, Some(true));
    assert_eq!(models[0].thinking_adaptive_supported, Some(false));
    assert_eq!(models[0].thinking_enabled_supported, Some(true));
}

#[cfg(all(
    not(target_arch = "wasm32"),
    feature = "cache-memory",
    feature = "persist-db",
    feature = "channel-openai"
))]
mod fetch {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use http::header::AUTHORIZATION;

    use crate::app::AppState;
    use crate::app::snapshot::ControlPlaneSnapshot;
    use crate::channel::PreparedRequest;
    use crate::config::{
        CacheConfig, DEFAULT_MAX_ATTEMPTS, DEFAULT_MAX_IN_FLIGHT, PersistenceConfig, RuntimeConfig,
        UpstreamConfig,
    };
    use crate::health::CredAdmit;
    use crate::http::client::ClientError;

    const BUNDLE: &str = r#"{
      "schema_version": 1,
      "providers": [
        { "id": 1, "name": "oai", "channel": "openai", "label": null,
          "settings_json": { "endpoints": { "openai_list_models": "http://fake.local/v1/models" } },
          "credential_strategy": "round_robin", "proxy_url": null,
          "tls_fingerprint": null, "enabled": true }
      ],
      "credentials": [
        { "id": 1, "provider_id": 1, "label": "bad",
          "secret_json": { "api_key": "bad-key" }, "enabled": true },
        { "id": 2, "provider_id": 1, "label": "good",
          "secret_json": { "api_key": "good-key" }, "enabled": true },
        { "id": 3, "provider_id": 1, "label": "other",
          "secret_json": { "api_key": "other-key" }, "enabled": true }
      ]
    }"#;

    struct Seen {
        uri: String,
        authorization: Option<String>,
    }

    struct SequencedUpstream {
        statuses: Vec<StatusCode>,
        seen: Mutex<Vec<Seen>>,
        calls: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl UpstreamClient for SequencedUpstream {
        async fn send(
            &self,
            req: http::Request<Bytes>,
        ) -> Result<http::Response<Bytes>, ClientError> {
            let authorization = req
                .headers()
                .get(AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            self.seen.lock().unwrap().push(Seen {
                uri: req.uri().to_string(),
                authorization: authorization.clone(),
            });
            let i = self.calls.fetch_add(1, Ordering::SeqCst);
            let status = self
                .statuses
                .get(i)
                .or_else(|| self.statuses.last())
                .copied()
                .unwrap_or(StatusCode::OK);
            let body = if !status.is_success() {
                Bytes::from_static(br#"{"error":"bad credential"}"#)
            } else if authorization.as_deref() == Some("Bearer good-key") {
                Bytes::from_static(
                    br#"{"object":"list","data":[{"id":"shared"},{"id":"gpt-good"}]}"#,
                )
            } else {
                Bytes::from_static(
                    br#"{"object":"list","data":[{"id":"shared"},{"id":"gpt-other"}]}"#,
                )
            };
            Ok(http::Response::builder()
                .status(status)
                .header("content-type", "application/json")
                .body(body)
                .expect("response"))
        }
    }

    impl SequencedUpstream {
        fn new(statuses: Vec<StatusCode>) -> Self {
            Self {
                statuses,
                seen: Mutex::new(Vec::new()),
                calls: AtomicUsize::new(0),
            }
        }
    }

    struct CustomListChannel;

    #[async_trait::async_trait]
    impl Channel for CustomListChannel {
        fn id(&self) -> &'static str {
            "custom-list-test"
        }

        fn routing_table(&self) -> crate::channel::routes::RouteList {
            use crate::channel::routes::{pass, pv};

            vec![pass(Operation::ListModels, pv(Provider::OpenAi))]
        }

        fn prepare(&self, _ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
            let request = http::Request::get("http://fake.local/v1/models")
                .body(Bytes::new())
                .unwrap();
            Ok(PreparedRequest::custom(Box::new(move |client| {
                Box::pin(async move { client.send(request).await })
            })))
        }
    }

    async fn state_with(upstream: Arc<SequencedUpstream>) -> (AppState, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let persistence: Arc<dyn crate::store::persistence::PersistenceBackend> = Arc::new(
            crate::store::persistence::DbPersistence::connect("sqlite::memory:")
                .await
                .expect("db persistence"),
        );
        crate::app::import::import_bundle(persistence.as_ref(), &crate::crypto::NoopCipher, BUNDLE)
            .await
            .expect("import");
        let snapshot = ControlPlaneSnapshot::build(persistence.as_ref(), 1)
            .await
            .expect("snapshot");
        let config = Arc::new(RuntimeConfig {
            host: "127.0.0.1".into(),
            port: 0,
            cache: CacheConfig::Memory,
            persistence: PersistenceConfig::Db {
                dsn: "sqlite::memory:".to_string(),
            },
            upstream: UpstreamConfig::from_proxy_url(None),
            instance_id: 0,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            max_in_flight: DEFAULT_MAX_IN_FLIGHT,
            trusted_proxies: Vec::new(),
            update_channel: "releases".to_string(),
            update_data_dir: dir.path().to_path_buf(),
            cors_origins: Vec::new(),
        });
        let cache: Arc<dyn crate::store::cache::CacheBackend> =
            Arc::new(crate::store::cache::MemoryCache::new());
        let upstream_client: Arc<dyn UpstreamClient> = upstream;
        let state = AppState::new(
            config,
            cache,
            persistence,
            upstream_client,
            Arc::new(arc_swap::ArcSwap::from_pointee(snapshot)),
            Arc::new(crate::channel::registry::ChannelRegistry::with_builtin()),
            Arc::new(crate::crypto::NoopCipher),
        );
        (state, dir)
    }

    #[tokio::test]
    async fn fetch_models_unions_all_successful_credentials_after_partial_failure() {
        let upstream = Arc::new(SequencedUpstream::new(vec![
            StatusCode::UNAUTHORIZED,
            StatusCode::OK,
            StatusCode::OK,
        ]));
        let (state, _dir) = state_with(Arc::clone(&upstream)).await;

        let models = fetch_models(&state, 1).await.expect("model pull");
        assert_eq!(
            models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["shared", "gpt-good", "gpt-other"]
        );

        let seen = upstream.seen.lock().unwrap();
        assert_eq!(seen.len(), 3, "all credentials are pulled serially");
        assert_eq!(seen[0].uri, "http://fake.local/v1/models");
        assert_eq!(
            seen.iter()
                .map(|s| s.authorization.as_deref())
                .collect::<Vec<_>>(),
            [
                Some("Bearer bad-key"),
                Some("Bearer good-key"),
                Some("Bearer other-key")
            ]
        );
        let now = crate::util::time::unix_now();
        assert_eq!(state.health.credential_available(1, now), CredAdmit::No);
        assert_eq!(
            state.health.credential_model_available(1, "any-model", now),
            CredAdmit::No
        );
    }

    #[tokio::test]
    async fn fetch_models_returns_error_when_every_credential_fails() {
        let upstream = Arc::new(SequencedUpstream::new(vec![StatusCode::UNAUTHORIZED]));
        let (state, _dir) = state_with(Arc::clone(&upstream)).await;

        let err = fetch_models(&state, 1)
            .await
            .expect_err("all credentials fail");
        assert!(matches!(err, ModelsError::Status(401)));
        assert_eq!(
            upstream.seen.lock().unwrap().len(),
            3,
            "each credential is attempted once"
        );
    }

    #[tokio::test]
    async fn buffered_model_pull_executes_custom_exchange_with_resolved_client() {
        let upstream = Arc::new(SequencedUpstream::new(vec![StatusCode::OK]));
        let client: Arc<dyn UpstreamClient> = upstream.clone();
        let channel: Arc<dyn Channel> = Arc::new(CustomListChannel);
        let secret = serde_json::json!({});
        let settings = serde_json::json!({});

        let result = fetch_models_with(
            &channel,
            OperationKey::provider(Operation::ListModels, Provider::OpenAi),
            &secret,
            &settings,
            &client,
        )
        .await
        .unwrap();
        let ModelPullResult::Success(models) = result else {
            panic!("custom model pull should succeed");
        };

        assert_eq!(
            models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            ["shared", "gpt-other"]
        );
        assert_eq!(upstream.calls.load(Ordering::SeqCst), 1);
    }

    #[cfg(feature = "upstream-wreq")]
    #[tokio::test]
    async fn provider_scoped_client_applies_fingerprint_fail_closed() {
        let upstream = Arc::new(SequencedUpstream::new(vec![StatusCode::OK]));
        let (state, _dir) = state_with(upstream).await;
        let mut provider = state.cp().providers_by_id[&1].as_ref().clone();
        provider.tls_fingerprint = Some(serde_json::json!({ "_note": "not an emulation" }));

        assert!(matches!(
            state.upstream_client_for_provider(&provider),
            Err(ClientError::Config(_))
        ));
    }
}
