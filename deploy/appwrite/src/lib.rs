//! gproxy on **Appwrite Functions** (Rust 1.83 / open-runtimes runtime).
//!
//! Appwrite's Rust runtime calls a synchronous `pub fn main(context) -> Response`.
//! This adapter bridges that request/response model onto gproxy's *native* axum
//! router — the SAME router the standalone binary serves — by driving one
//! request through it with `tower::ServiceExt::oneshot`:
//!
//!   Appwrite `context.req`  ->  `http::Request`  ->  gproxy router  ->
//!   `http::Response`        ->  Appwrite `Response`
//!
//! `AppState` is built ONCE per function instance (on the first invocation /
//! cold start) and reused. Config comes from the function's environment
//! variables (set in the Appwrite Console → Function → Settings):
//!
//!   GPROXY_DSN                 (required) Postgres/MySQL DSN for the control
//!                              plane — serverless has no durable local disk, so
//!                              file persistence is not an option here.
//!   GPROXY_MASTER_KEY          (optional) unseals stored secrets; absent =
//!                              plaintext mode.
//!   GPROXY_UPSTREAM_PROXY_URL  (optional) outbound proxy for upstream calls.
//!   GPROXY_ADMIN_USER          (optional, default "admin") first-boot admin.
//!   GPROXY_ADMIN_PASSWORD      (optional) first-boot admin password.
//!
//! Status: the adapter COMPILES against the real gproxy router + open-runtimes
//! types. End-to-end behaviour on Appwrite is NOT yet verified (no account in
//! this environment); see NOTES.md for the open feasibility questions
//! (BoringSSL/onig build in the rust-1.83 container, path-dep packaging).

use std::sync::Arc;
use std::sync::OnceLock;

use openruntimes::{Context, Response};

use gproxy::app::AppState;
use gproxy::config::{
    CacheConfig, PersistenceConfig, RuntimeConfig, UpstreamConfig, DEFAULT_MAX_ATTEMPTS,
    DEFAULT_MAX_IN_FLIGHT,
};
use gproxy::http::client::{UpstreamClient, WreqClient};
use gproxy::store::cache::{CacheBackend, MemoryCache};
use gproxy::store::persistence::{DbPersistence, PersistenceBackend};

/// Multi-thread Tokio runtime, built once and reused to drive the async router
/// from the synchronous handler.
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build tokio runtime")
    })
}

/// Build (or return the already-built) shared application state for this
/// function instance.
fn state() -> anyhow::Result<&'static AppState> {
    static STATE: OnceLock<AppState> = OnceLock::new();
    if let Some(s) = STATE.get() {
        return Ok(s);
    }
    let built = runtime().block_on(build_state())?;
    // Another invocation may have raced us; either way a state ends up stored.
    let _ = STATE.set(built);
    Ok(STATE.get().expect("state initialised"))
}

async fn build_state() -> anyhow::Result<AppState> {
    let dsn = std::env::var("GPROXY_DSN")
        .map_err(|_| anyhow::anyhow!("GPROXY_DSN is required (Postgres/MySQL DSN)"))?;
    let proxy = std::env::var("GPROXY_UPSTREAM_PROXY_URL").ok();
    let master_key = std::env::var("GPROXY_MASTER_KEY").ok();

    let cipher = gproxy::crypto::cipher_from_master_key(master_key.as_deref())?;

    let persistence: Arc<dyn PersistenceBackend> = Arc::new(DbPersistence::connect(&dsn).await?);
    persistence.health().await?;

    let admin_user = std::env::var("GPROXY_ADMIN_USER").unwrap_or_else(|_| "admin".to_string());
    let admin_password = std::env::var("GPROXY_ADMIN_PASSWORD").ok();
    gproxy::app::bootstrap::ensure_admin(persistence.as_ref(), &admin_user, admin_password.as_deref())
        .await?;

    let cache: Arc<dyn CacheBackend> = Arc::new(MemoryCache::new());
    let upstream: Arc<dyn UpstreamClient> = Arc::new(WreqClient::with_proxy_url(proxy.as_deref())?);

    let snapshot =
        gproxy::app::snapshot::ControlPlaneSnapshot::build(persistence.as_ref(), 1).await?;
    let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(snapshot));
    let channels = Arc::new(gproxy::channel::registry::ChannelRegistry::with_builtin());

    let config = Arc::new(RuntimeConfig {
        host: "0.0.0.0".to_string(),
        port: 0,
        cache: CacheConfig::Memory,
        persistence: PersistenceConfig::Db { dsn },
        upstream: UpstreamConfig::from_proxy_url(proxy),
        instance_id: 0,
        max_attempts: DEFAULT_MAX_ATTEMPTS,
        max_in_flight: DEFAULT_MAX_IN_FLIGHT,
        trusted_proxies: Vec::new(),
        update_repo: None,
        update_channel: "releases".to_string(),
        update_data_dir: std::path::PathBuf::from("./data"),
        cors_origins: Vec::new(),
    });

    Ok(AppState::new(
        config,
        cache,
        persistence,
        upstream,
        snapshot,
        channels,
        cipher,
    ))
}

/// Appwrite Functions entry point (open-runtimes Rust contract).
pub fn main(context: Context) -> Response {
    let state = match state() {
        Ok(s) => s,
        Err(e) => {
            context.error(format!("gproxy init failed: {e}"));
            return context.res.text(format!("gproxy init error: {e}"), Some(500), None);
        }
    };

    let req = match to_http_request(&context.req) {
        Ok(r) => r,
        Err(e) => return context.res.text(format!("bad request: {e}"), Some(400), None),
    };

    let result = runtime().block_on(async {
        use tower::util::ServiceExt;
        let resp = gproxy::http::server::router(state.clone())
            .oneshot(req)
            .await
            .map_err(|e| anyhow::anyhow!("router error: {e}"))?;
        let status = resp.status().as_u16();
        let headers: std::collections::HashMap<String, String> = resp
            .headers()
            .iter()
            .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.as_str().to_string(), v.to_string())))
            .collect();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await?.to_vec();
        Ok::<_, anyhow::Error>((status, headers, body))
    });

    match result {
        Ok((status, headers, body)) => context.res.binary(body, Some(status), Some(headers)),
        Err(e) => {
            context.error(format!("{e}"));
            context.res.text(format!("{e}"), Some(502), None)
        }
    }
}

/// Convert the Appwrite request into an `http::Request` for the axum router.
fn to_http_request(
    req: &openruntimes::ContextRequest,
) -> anyhow::Result<http::Request<axum::body::Body>> {
    let uri = if req.query_string.is_empty() {
        req.path.clone()
    } else {
        format!("{}?{}", req.path, req.query_string)
    };

    let mut builder = http::Request::builder()
        .method(http::Method::from_bytes(req.method.as_bytes())?)
        .uri(uri);
    for (k, v) in &req.headers {
        builder = builder.header(k, v);
    }
    // The native router's trusted-proxy layer reads ConnectInfo; supply a
    // placeholder peer (the real client IP arrives via forwarding headers).
    let mut request = builder.body(axum::body::Body::from(req.body_binary()))?;
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        std::net::SocketAddr::from(([0, 0, 0, 0], 0)),
    ));
    Ok(request)
}
