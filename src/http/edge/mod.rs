//! Edge inbound entry: bridges a WinterCG `fetch` event into the request
//! pipeline.
//!
//! `init` builds the shared [`AppState`] from JS-host-passed credentials
//! (libSQL/Turso persistence — schema ensured on connect — + Upstash/libSQL
//! cache + optional master key) and stashes it in a process-global `OnceLock`.
//! `fetch` then dispatches each request BY PATH directly to the same
//! [`pipeline`](crate::pipeline) / [`metrics`](crate::http::server::metrics)
//! code native uses — NOT through the axum router, whose `Handler` requires
//! `Send` futures the wasm gateway path (FetchClient / libSQL) cannot satisfy.
//!
//! `init` MUST be called exactly once before the first `fetch`. If `fetch`
//! runs before `init`, it returns a 503 with a clear message (an `AppState`
//! cannot be synthesised inside wasm without host-supplied credentials).

pub(crate) mod http;

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use bytes::Bytes;
use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Response, ResponseInit};
// Explicit imports from the `http` external crate (the `mod http;` submodule
// below shadows the bare `http::` path — use `::http::` or import here).
use ::http::{HeaderMap, Method, Uri, header::HeaderName};

use crate::app::AppState;
use crate::config::{CacheConfig, PersistenceConfig, RuntimeConfig, UpstreamConfig};
use crate::http::client::{FetchClient, UpstreamClient};
use crate::store::cache::{CacheBackend, LibsqlCache, UpstashCache};
use crate::store::persistence::{LibsqlPersistence, PersistenceBackend};

/// Process-global app state, populated once by [`init`].
static STATE: OnceLock<AppState> = OnceLock::new();

/// §7.2 edge snapshot freshness: minimum interval between config-version
/// polls. Within the window requests serve the current snapshot untouched.
const SNAPSHOT_POLL_INTERVAL_MS: u64 = 10_000;

/// Wall-clock millis of this isolate's last config-version poll.
static LAST_POLL_MS: AtomicU64 = AtomicU64::new(0);
/// Config version this isolate's snapshot was last built against.
static SEEN_CFG_VERSION: AtomicI64 = AtomicI64::new(0);

fn js_err(e: impl std::fmt::Debug) -> JsValue {
    JsValue::from_str(&format!("{e:?}"))
}

/// Initialise the edge runtime from host-supplied credentials.
///
/// Persistence is always libSQL/Turso (`turso_url` + `turso_token`). The cache
/// is Upstash Redis when both `upstash_url` and `upstash_token` are non-empty,
/// otherwise it falls back to the libSQL kv table. `master_key` unseals stored
/// secrets (absent → plaintext NoopCipher).
///
/// Must be called once before [`fetch`]. A second call is a no-op (the first
/// `AppState` wins).
#[wasm_bindgen]
pub async fn init(
    turso_url: String,
    turso_token: String,
    upstash_url: Option<String>,
    upstash_token: Option<String>,
    master_key: Option<String>,
) -> Result<(), JsValue> {
    if STATE.get().is_some() {
        return Ok(());
    }

    // `connect` also ensures the schema (CREATE TABLE IF NOT EXISTS), so an
    // empty edge-first Turso database is usable immediately.
    let persistence: Arc<dyn PersistenceBackend> = Arc::new(
        LibsqlPersistence::connect(turso_url.clone(), turso_token.clone())
            .await
            .map_err(js_err)?,
    );

    let (cache, cache_cfg): (Arc<dyn CacheBackend>, CacheConfig) =
        match (upstash_url, upstash_token) {
            (Some(u), Some(t)) if !u.is_empty() && !t.is_empty() => (
                Arc::new(UpstashCache::new(u.clone(), t)),
                CacheConfig::Upstash { url: u },
            ),
            _ => {
                let c = LibsqlCache::connect(turso_url.clone(), turso_token.clone())
                    .await
                    .map_err(js_err)?;
                (
                    Arc::new(c),
                    CacheConfig::Libsql {
                        url: turso_url.clone(),
                    },
                )
            }
        };

    let config = Arc::new(RuntimeConfig {
        host: "0.0.0.0".to_string(),
        port: 0,
        cache: cache_cfg,
        persistence: PersistenceConfig::Db { dsn: turso_url },
        upstream: UpstreamConfig::from_proxy_url(None),
        instance_id: 0,
        max_attempts: crate::config::DEFAULT_MAX_ATTEMPTS,
        max_in_flight: crate::config::DEFAULT_MAX_IN_FLIGHT,
        trusted_proxies: Vec::new(),
        update_repo: None,
        update_channel: "releases".to_string(),
        // Edge (wasm) never self-updates; PathBuf is required by the type but
        // the field is never read in an edge build.
        update_data_dir: std::path::PathBuf::from("./data"),
        cors_origins: Vec::new(),
    });

    let upstream: Arc<dyn UpstreamClient> = Arc::new(FetchClient::new());

    // Build the control-plane snapshot from persistence (libSQL read ops). An
    // un-provisioned database yields an empty snapshot; provisioning via the
    // admin API or `import` (into the same Turso DB) makes routing live.
    let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(
        crate::app::snapshot::ControlPlaneSnapshot::build(persistence.as_ref(), 1)
            .await
            .map_err(js_err)?,
    ));
    let channels = Arc::new(crate::channel::registry::ChannelRegistry::with_builtin());

    // §7.2 baseline: remember the config version this snapshot was built at so
    // the first request doesn't trigger a spurious rebuild (incr-by-0 reads
    // the counter, creating it at 0 when absent). An unreadable stamp
    // baselines at 0 — the first successful poll then rebuilds once (safe
    // direction).
    SEEN_CFG_VERSION.store(
        cache
            .incr(crate::store::cache::CONFIG_VERSION_KEY, 0, None)
            .await
            .unwrap_or(0),
        Ordering::Relaxed,
    );
    LAST_POLL_MS.store(js_sys::Date::now() as u64, Ordering::Relaxed);

    let _ = STATE.set(AppState::new(
        config,
        cache,
        persistence,
        upstream,
        snapshot,
        channels,
        // Host-supplied master key (base64) unseals stored secrets; absent →
        // NoopCipher (plaintext), for a plaintext-secret edge deployment.
        crate::crypto::cipher_from_master_key(master_key.as_deref()).map_err(js_err)?,
    ));
    Ok(())
}

/// WinterCG fetch entry-point: receives an inbound Request, dispatches it
/// through the SAME pipeline native uses — directly, NOT via the axum router.
/// axum 0.8's `Handler` requires `Send` futures, which the wasm gateway path
/// (FetchClient / libSQL) is not; so the edge routes by path here and calls
/// [`pipeline::execute`] / [`metrics`](crate::http::server::metrics) itself.
///
/// Returns 503 if [`init`] has not yet been called.
#[wasm_bindgen]
pub async fn fetch(req: web_sys::Request) -> Result<Response, JsValue> {
    let Some(state) = STATE.get() else {
        return service_unavailable("GPROXY edge not initialised: call init() first");
    };

    // §7.2 lazy snapshot refresh: edge has no pub/sub listener, so poll the
    // shared config-version stamp (throttled) and rebuild when it moved.
    refresh_snapshot_if_stale(state).await;

    // Body cap (shared with native's DefaultBodyLimit): reject via
    // content-length BEFORE buffering when the header is present…
    if content_length_exceeds(&req, crate::config::MAX_BODY_BYTES) {
        return payload_too_large();
    }
    let (parts, body) = ws_request_to_parts(req).await?;
    // …and re-check the actual buffered length (content-length can be absent
    // or lying). Both produce a clean 413, not a JS exception.
    if body.len() > crate::config::MAX_BODY_BYTES {
        return payload_too_large();
    }
    let path = parts.uri.path().to_string();

    // Operational endpoints: no pipeline, no upstream. /healthz, /version and
    // /metrics all sit behind the SAME admin auth as /admin/* (session cookie
    // or an admin user's API key — see admin_ok); none is public. Bodies match
    // the native axum handlers byte-for-byte (JSON — see server::health).
    match path.as_str() {
        "/healthz" => {
            return if admin_ok(state, &parts.headers).await {
                text_response(200, "application/json", br#"{"status":"ok"}"#)
            } else {
                unauthorized()
            };
        }
        "/version" => {
            return if admin_ok(state, &parts.headers).await {
                const VERSION_JSON: &str =
                    concat!(r#"{"version":""#, env!("CARGO_PKG_VERSION"), r#""}"#);
                text_response(200, "application/json", VERSION_JSON.as_bytes())
            } else {
                unauthorized()
            };
        }
        "/metrics" => {
            if !admin_ok(state, &parts.headers).await {
                return unauthorized();
            }
            return match state.persistence.metrics_aggregate().await {
                Ok(agg) => text_response(
                    200,
                    "text/plain; version=0.0.4",
                    crate::http::server::metrics::render(&agg).as_bytes(),
                ),
                Err(e) => {
                    tracing::warn!(error = %e, "metrics aggregate failed");
                    text_response(500, "text/plain", b"metrics unavailable")
                }
            };
        }
        _ => {}
    }

    // Admin control-plane + portal: `/admin/*` and `/user/*` are served by the
    // cross-target dispatcher (auth guard + CSRF + thin persistence glue), which
    // returns pure `Resp` data we convert to a `web_sys::Response` here. A path
    // under these prefixes the dispatcher does not handle → 404 (never falls
    // through to the gateway).
    if path.starts_with("/admin/") || path.starts_with("/user/") {
        return match crate::http::admin_api::dispatch(state, &parts, &body).await {
            Some(Ok(resp)) => resp_to_ws(resp),
            Some(Err(e)) => self::http::api_err_response(&e),
            None => self::http::api_err_response(&crate::api::error::ApiError::NotFound(
                "not found".into(),
            )),
        };
    }

    // Gateway: `/v1/...` and gemini's `/v1beta/...` are aggregated; anything
    // else is `/{provider}/v1[beta]/...` scoped (build_ctx validates and
    // rejects malformed paths).
    let scoped = !(path == "/v1"
        || path.starts_with("/v1/")
        || path == "/v1beta"
        || path.starts_with("/v1beta/"));
    let ctx = match crate::http::server::extract::build_ctx(parts, body, scoped) {
        Ok(c) => c,
        Err(e) => return error_to_ws(&e),
    };
    let request_id = ctx.request_id.clone();
    match crate::pipeline::execute(state, ctx).await {
        Ok(outcome) => outcome_to_ws(outcome, &request_id),
        Err(e) => error_to_ws(&e),
    }
}

/// Build a 503 (init-not-called) plain-text response.
fn service_unavailable(msg: &str) -> Result<Response, JsValue> {
    text_response(503, "text/plain", msg.as_bytes())
}

/// Convert the cross-target [`Resp`](crate::http::admin_api::Resp) returned by
/// the admin/portal dispatcher into a `web_sys::Response` (status + headers +
/// body). The dispatcher owns the status/headers/body as pure data; this is the
/// only wasm-specific step.
fn resp_to_ws(resp: crate::http::admin_api::Resp) -> Result<Response, JsValue> {
    let headers = Headers::new().map_err(js_err)?;
    for (name, value) in &resp.headers {
        if let Ok(v) = value.to_str() {
            headers.append(name.as_str(), v).map_err(js_err)?;
        }
    }
    js_response(resp.status.as_u16(), &headers, &resp.body)
}

/// Edge replacement for the native pub/sub invalidation listener (§7.2): at
/// most once per [`SNAPSHOT_POLL_INTERVAL_MS`], read the config-version stamp
/// [`broadcast`](crate::app::invalidation::broadcast) bumps on every mutation
/// and rebuild the snapshot when it moved. The poll slot is claimed BEFORE the
/// awaits so interleaved requests (single-threaded event loop, but suspension
/// points interleave) don't duplicate the work; a failed rebuild leaves the
/// seen version untouched so the next window retries.
async fn refresh_snapshot_if_stale(state: &AppState) {
    let now_ms = js_sys::Date::now() as u64;
    if now_ms.saturating_sub(LAST_POLL_MS.load(Ordering::Relaxed)) < SNAPSHOT_POLL_INTERVAL_MS {
        return;
    }
    LAST_POLL_MS.store(now_ms, Ordering::Relaxed);
    let version = match state
        .cache
        .incr(crate::store::cache::CONFIG_VERSION_KEY, 0, None)
        .await
    {
        Ok(v) => v,
        // Stamp unreadable: keep serving the current snapshot; the next poll
        // window retries.
        Err(_) => return,
    };
    if version == SEEN_CFG_VERSION.load(Ordering::Relaxed) {
        return;
    }
    match state.reload_snapshot().await {
        Ok(()) => {
            SEEN_CFG_VERSION.store(version, Ordering::Relaxed);
            tracing::info!(version, "edge snapshot refreshed");
        }
        Err(e) => tracing::warn!(error = %e, "edge snapshot refresh failed"),
    }
}

/// Build the 401 for missing/invalid admin auth on an ops endpoint.
fn unauthorized() -> Result<Response, JsValue> {
    text_response(401, "text/plain", b"unauthorized")
}

/// Build the 413 for an over-cap request body.
fn payload_too_large() -> Result<Response, JsValue> {
    text_response(413, "text/plain", b"request body too large")
}

/// Shared `/healthz` + `/version` + `/metrics` gate — the SAME auth as the
/// native `/admin/*` middleware ([`authenticate_admin`](crate::admin::authenticate_admin)):
/// admin session cookie (minted by a native instance / console sharing the
/// same Turso+Upstash backing) OR an admin user's API key.
async fn admin_ok(state: &AppState, headers: &HeaderMap) -> bool {
    crate::admin::authenticate_admin(state, headers)
        .await
        .is_some()
}

/// `true` when a present, parseable `content-length` already exceeds `max`.
/// Pre-read fast-fail only — the authoritative check in [`fetch`] measures the
/// buffered body, since the header can be absent or lying.
fn content_length_exceeds(req: &web_sys::Request, max: usize) -> bool {
    matches!(
        req.headers().get("content-length"),
        Ok(Some(v)) if v.trim().parse::<u64>().is_ok_and(|n| n > max as u64)
    )
}

/// Build a response with a single `Content-Type` header and a body.
pub(super) fn text_response(
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<Response, JsValue> {
    let headers = Headers::new().map_err(js_err)?;
    headers
        .append("content-type", content_type)
        .map_err(js_err)?;
    js_response(status, &headers, body)
}

/// Render a [`PipelineError`](crate::pipeline::error::PipelineError) to a JSON
/// error response, redacted identically to the native axum surface.
fn error_to_ws(e: &crate::pipeline::error::PipelineError) -> Result<Response, JsValue> {
    let headers = Headers::new().map_err(js_err)?;
    headers
        .append("content-type", "application/json")
        .map_err(js_err)?;
    if let Some(secs) = e.retry_after_secs() {
        headers
            .append("retry-after", &secs.to_string())
            .map_err(js_err)?;
    }
    js_response(e.status().as_u16(), &headers, e.error_json().as_bytes())
}

/// Map an [`ExecOutcome`](crate::pipeline::outcome::ExecOutcome) to a Response:
/// status + hop-by-hop-sanitized headers + the buffered body + the request id.
/// On wasm the body is always `Full` (the streaming variant is native-only).
fn outcome_to_ws(
    outcome: crate::pipeline::outcome::ExecOutcome,
    request_id: &str,
) -> Result<Response, JsValue> {
    use crate::channel::http_util::sanitize_response_headers;
    use crate::pipeline::outcome::ResponseBody;

    let sanitized = sanitize_response_headers(&outcome.headers);
    let headers = Headers::new().map_err(js_err)?;
    for (name, value) in &sanitized {
        if let Ok(v) = value.to_str() {
            headers.append(name.as_str(), v).map_err(js_err)?;
        }
    }
    headers
        .append("x-gproxy-request-id", request_id)
        .map_err(js_err)?;

    let ResponseBody::Full(bytes) = outcome.body;
    js_response(outcome.status.as_u16(), &headers, &bytes)
}

/// Core response builder: status + headers + a JS-OWNED body copy.
///
/// The body is copied into a JS-owned `Uint8Array` (its own ArrayBuffer) rather
/// than handing `new Response(...)` a `&mut [u8]` view into wasm linear memory.
/// Vercel's Edge Runtime retains that view lazily, so a later wasm allocation
/// detaches/overwrites the buffer and the body comes out garbled or never
/// completes. Deno copies eagerly so a view worked there, but an owned copy is
/// correct everywhere.
pub(super) fn js_response(
    status: u16,
    headers: &Headers,
    body: &[u8],
) -> Result<Response, JsValue> {
    let init = ResponseInit::new();
    init.set_status(status);
    init.set_headers_headers(headers);
    let js_body = Uint8Array::new_with_length(body.len() as u32);
    js_body.copy_from(body);
    Response::new_with_opt_js_u8_array_and_init(Some(&js_body), &init).map_err(js_err)
}

/// Convert `web_sys::Request` → `(::http::request::Parts, Bytes)`.
async fn ws_request_to_parts(
    req: web_sys::Request,
) -> Result<(::http::request::Parts, Bytes), JsValue> {
    let method = Method::from_bytes(req.method().as_bytes()).map_err(js_err)?;
    let uri: Uri = req.url().parse().map_err(js_err)?;

    // Read body via array_buffer.
    let body_bytes: Bytes = {
        let buf_promise = req.array_buffer().map_err(js_err)?;
        let buf_val = JsFuture::from(buf_promise).await.map_err(js_err)?;
        Uint8Array::new(&buf_val).to_vec().into()
    };

    // Copy headers; skip empty/unparseable names so a bad header can't poison
    // the whole builder.
    let mut builder = ::http::Request::builder().method(method).uri(uri);
    let ws_headers = req.headers();
    if let Some(iter) = js_sys::try_iter(&ws_headers).map_err(js_err)? {
        for entry in iter {
            let entry = entry.map_err(js_err)?;
            let arr: js_sys::Array = entry.unchecked_into();
            let name = arr.get(0).as_string().unwrap_or_default();
            let val = arr.get(1).as_string().unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            if let Ok(hn) = HeaderName::try_from(name.as_str()) {
                builder = builder.header(hn, val.as_str());
            }
        }
    }

    let (parts, _) = builder.body(()).map_err(js_err)?.into_parts();
    Ok((parts, body_bytes))
}
