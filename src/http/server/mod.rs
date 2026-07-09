//! HTTP surface. Domain routers (admin, console) get nested here in later phases.

use axum::Router;
use axum::routing::get;

use crate::app::AppState;

mod health;
pub mod metrics;

// The gateway request path is `?Send` on wasm (FetchClient / libSQL), which axum
// 0.8's `Handler` (requires `Send` futures) rejects. Native wires the gateway as
// axum handlers; the edge fetch entry (`http::edge`) calls the same pipeline
// directly via `extract::build_ctx` + `pipeline::execute`, bypassing the router.
// `extract` is pure (http types only), so it compiles on both targets.
pub mod extract;
#[cfg(not(target_arch = "wasm32"))]
mod gateway;

#[cfg(not(target_arch = "wasm32"))]
pub mod admin;

#[cfg(not(target_arch = "wasm32"))]
mod console;

/// Build the top-level axum router.
///
/// On native the literal `/v1/...` aggregated route is registered before the
/// `/{provider}/v1/...` scoped route; the scoped handler additionally rejects
/// `provider == "v1"` and `provider == "console"`, so both `v1` and `console`
/// are reserved as non-provider segments.
pub fn router(state: AppState) -> Router {
    #[allow(unused_mut)]
    let mut router = Router::new();

    // wasm builds this router for type-compatibility only — the edge entry
    // (http::edge) dispatches by path and never serves it; it admin-gates
    // /healthz + /version + /metrics itself, so plain registrations here just
    // keep the handlers live on both targets.
    #[cfg(target_arch = "wasm32")]
    {
        router = router
            .route("/healthz", get(health::healthz))
            .route("/version", get(health::version));
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        use axum::error_handling::HandleErrorLayer;
        use axum::extract::DefaultBodyLimit;
        use axum::routing::any;
        use tower::ServiceBuilder;
        use tower::limit::GlobalConcurrencyLimitLayer;
        use tower::load_shed::LoadShedLayer;

        // Gateway sub-router with §16.2 overload protection: at most
        // `max_in_flight` concurrent requests; excess is shed to 503 immediately
        // (not queued). Scoped to the gateway only — health / metrics / admin
        // stay reachable under load so liveness holds and operators can intervene.
        let mut gateway = Router::new()
            .route("/v1/{*rest}", any(gateway::aggregated))
            .route("/{provider}/v1/{*rest}", any(gateway::scoped))
            // Gemini speaks `/v1beta/...` rather than `/v1/...`; register the
            // parallel surface so the gemini inbound spec reaches `classify`
            // (which already handles these paths) instead of a router 404.
            .route("/v1beta/{*rest}", any(gateway::aggregated))
            .route("/{provider}/v1beta/{*rest}", any(gateway::scoped))
            .layer(DefaultBodyLimit::max(crate::config::MAX_BODY_BYTES))
            .layer(
                ServiceBuilder::new()
                    .layer(HandleErrorLayer::new(handle_overload))
                    .layer(LoadShedLayer::new())
                    .layer(GlobalConcurrencyLimitLayer::new(state.config.max_in_flight)),
            );
        if !state.config.cors_origins.is_empty() {
            gateway = gateway.layer(crate::http::cors::credentialed_gateway_layer(
                &state.config.cors_origins,
            ));
        }
        router = router.merge(gateway);
        // /healthz, /version and /metrics sit behind the SAME admin gate as
        // /admin/* (session cookie or an admin user's API key, via
        // require_admin) — no ops endpoint is public.
        let ops = Router::new()
            .route("/healthz", get(health::healthz))
            .route("/version", get(health::version))
            .route("/metrics", get(metrics::metrics))
            .route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                admin::middleware::require_admin,
            ));
        router = router.merge(ops);
        router = router.merge(admin::admin_router(state.clone()));
        // Console SPA — public routes (the login page must load pre-auth); the
        // data it fetches is gated by /admin/* auth, not by asset serving.
        router = router.merge(console::router());
    }

    router.with_state(state)
}

/// Map a shed (overloaded) gateway request to a 503; any other middleware error
/// to a 500. Used by the §16.2 load-shed layer.
#[cfg(not(target_arch = "wasm32"))]
async fn handle_overload(err: tower::BoxError) -> axum::http::StatusCode {
    use axum::http::StatusCode;
    if err.is::<tower::load_shed::error::Overloaded>() {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{HeaderValue, Request, StatusCode, header};
    use tower::ServiceExt as _;

    use crate::app::AppState;
    use crate::app::snapshot::ControlPlaneSnapshot;
    use crate::config::{CacheConfig, PersistenceConfig, RuntimeConfig, UpstreamConfig};
    use crate::http::client::{ClientError, RespStream, UpstreamClient};
    use crate::store::persistence::FilePersistence;

    struct NoUpstream;

    #[async_trait::async_trait]
    impl UpstreamClient for NoUpstream {
        async fn send(
            &self,
            _req: http::Request<bytes::Bytes>,
        ) -> Result<http::Response<bytes::Bytes>, ClientError> {
            unreachable!("preflight must not call upstream")
        }

        async fn send_streaming(
            &self,
            _req: http::Request<bytes::Bytes>,
        ) -> Result<(StatusCode, http::HeaderMap, RespStream), ClientError> {
            unreachable!("preflight must not call upstream")
        }
    }

    async fn state_with_cors(cors_origins: Vec<String>) -> (AppState, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let persistence: Arc<dyn crate::store::persistence::PersistenceBackend> = Arc::new(
            FilePersistence::open(dir.path().to_path_buf())
                .await
                .expect("file persistence"),
        );
        let snapshot = ControlPlaneSnapshot::build(persistence.as_ref(), 1)
            .await
            .expect("snapshot");
        let config = Arc::new(RuntimeConfig {
            host: "127.0.0.1".into(),
            port: 0,
            cache: CacheConfig::Memory,
            persistence: PersistenceConfig::File {
                data_dir: dir.path().to_path_buf(),
            },
            upstream: UpstreamConfig::from_proxy_url(None),
            instance_id: 0,
            max_attempts: crate::config::DEFAULT_MAX_ATTEMPTS,
            max_in_flight: crate::config::DEFAULT_MAX_IN_FLIGHT,
            trusted_proxies: Vec::new(),
            update_channel: "releases".to_string(),
            update_data_dir: dir.path().to_path_buf(),
            cors_origins,
        });
        let cache: Arc<dyn crate::store::cache::CacheBackend> =
            Arc::new(crate::store::cache::MemoryCache::new());
        let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(snapshot));
        let channels = Arc::new(crate::channel::registry::ChannelRegistry::with_builtin());
        let state = AppState::new(
            config,
            cache,
            persistence,
            Arc::new(NoUpstream),
            snapshot,
            channels,
            Arc::new(crate::crypto::NoopCipher),
        );
        (state, dir)
    }

    #[tokio::test]
    async fn gateway_preflight_is_answered_before_auth_and_pipeline() {
        let (state, _dir) = state_with_cors(vec!["https://app.example".into()]).await;
        let app = super::router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/v1/chat/completions")
                    .header(header::ORIGIN, "https://app.example")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                    .header(
                        header::ACCESS_CONTROL_REQUEST_HEADERS,
                        "authorization,content-type,x-goog-api-key",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(&HeaderValue::from_static("https://app.example"))
        );
        assert_eq!(
            resp.headers().get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS),
            Some(&HeaderValue::from_static("true"))
        );
        assert_eq!(
            resp.headers().get(header::ACCESS_CONTROL_ALLOW_METHODS),
            Some(&HeaderValue::from_static("GET,POST,OPTIONS"))
        );
        assert_eq!(
            resp.headers().get(header::ACCESS_CONTROL_ALLOW_HEADERS),
            Some(&HeaderValue::from_static(
                "authorization,content-type,x-goog-api-key"
            ))
        );
    }
}
