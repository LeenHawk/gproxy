use super::*;

#[test]
fn parse_openai_and_gemini() {
    let oa = br#"{"object":"list","data":[{"id":"gpt-4o"},{"id":"gpt-4o-mini"}]}"#;
    let ids: Vec<_> = parse_models(Provider::OpenAi, oa)
        .into_iter()
        .map(|m| m.id)
        .collect();
    assert_eq!(ids, ["gpt-4o", "gpt-4o-mini"]);

    let gm = br#"{"models":[{"name":"models/gemini-1.5-pro","displayName":"Gemini 1.5 Pro"}]}"#;
    let g = parse_models(Provider::Gemini, gm);
    assert_eq!(g[0].id, "gemini-1.5-pro");
    assert_eq!(g[0].display_name.as_deref(), Some("Gemini 1.5 Pro"));
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
          "settings_json": { "base_url": "http://fake.local" },
          "credential_strategy": "round_robin", "proxy_url": null,
          "tls_fingerprint": null, "enabled": true }
      ],
      "credentials": [
        { "id": 1, "provider_id": 1, "label": "bad",
          "secret_json": { "api_key": "bad-key" }, "enabled": true },
        { "id": 2, "provider_id": 1, "label": "good",
          "secret_json": { "api_key": "good-key" }, "enabled": true }
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
                authorization,
            });
            let i = self.calls.fetch_add(1, Ordering::SeqCst);
            let status = self
                .statuses
                .get(i)
                .or_else(|| self.statuses.last())
                .copied()
                .unwrap_or(StatusCode::OK);
            let body = if status.is_success() {
                Bytes::from_static(br#"{"object":"list","data":[{"id":"gpt-good"}]}"#)
            } else {
                Bytes::from_static(br#"{"error":"bad credential"}"#)
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
    async fn fetch_models_tries_next_credential_after_auth_dead() {
        let upstream = Arc::new(SequencedUpstream::new(vec![
            StatusCode::UNAUTHORIZED,
            StatusCode::OK,
        ]));
        let (state, _dir) = state_with(Arc::clone(&upstream)).await;

        let models = fetch_models(&state, 1).await.expect("model pull");
        assert_eq!(
            models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["gpt-good"]
        );

        let seen = upstream.seen.lock().unwrap();
        assert_eq!(seen.len(), 2, "first auth-dead credential then next");
        assert_eq!(seen[0].uri, "http://fake.local/v1/models");
        assert_eq!(
            seen.iter()
                .map(|s| s.authorization.as_deref())
                .collect::<Vec<_>>(),
            [Some("Bearer bad-key"), Some("Bearer good-key")]
        );
        let now = crate::util::time::unix_now();
        assert_eq!(state.health.credential_available(1, now), CredAdmit::No);
        assert_eq!(
            state.health.credential_model_available(1, "any-model", now),
            CredAdmit::No
        );
    }
}
