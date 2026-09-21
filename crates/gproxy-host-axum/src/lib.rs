//! The native HTTP host: an axum router over [`gproxy_app::App`].
//!
//! # What this crate refuses to decide
//!
//! Every question of product behaviour has an answer somewhere below this
//! crate, and nothing here is allowed to give a second one. It does not decide
//! who a caller is ([`Authenticator`](gproxy_app::Authenticator)), what they
//! may reach ([`Admission`](gproxy_app::Admission)), which provider serves a
//! model (the sdk's resolver), what a failure is worth
//! ([`AppError::status_code`](gproxy_app::AppError::status_code)) or what an
//! operation does ([`Operations`](gproxy_app::Operations)). It decides how
//! bytes become those calls and how their answers become bytes: routing,
//! header and body decoding, CORS, the client address, the mount grammar, the
//! two error envelopes, streaming, and the lifetime of a rate-limit lease.
//!
//! That boundary is why a second host ([`gproxy-host-edge`]) can exist at all.
//! If anything product-shaped lived here, the edge deployment would have to
//! reimplement it and the two would drift.
//!
//! # The route table
//!
//! | Route | What it is |
//! |---|---|
//! | `GET /healthz` | liveness and the published revision |
//! | `GET /publications/{id}` | a published body, by its capability id |
//! | `/admin/api/*` | the operator surface, one `MethodRouter` per operation |
//! | `/portal/api/*` | the end user's surface, plus `login` and `logout` |
//! | everything else | [`ingress`], in the order that module documents |
//!
//! The management surfaces are explicit routes rather than a hand-rolled path
//! parser: axum decides the method, the path and the 404/405, and a typo in a
//! route is a compile-time missing handler instead of a silently unreachable
//! branch.
//!
//! # The lease-lifetime rule
//!
//! [`CallOutcome::admitted`](gproxy_app::CallOutcome) holds the request's
//! rate-limit charges, and a concurrency permit measures requests *in flight*.
//! A streamed response is in flight long after `App::call` returned, so the
//! host must hold the `Admitted` until the last byte has been written. This
//! crate does that by giving the response body a wrapper stream
//! ([`response::LeasedBody`]) that **owns** the decision: the lease, the
//! downstream capture and core's settlement future all live inside the stream
//! and are only released when it ends. Nothing has to remember to drop
//! anything, because there is nowhere else the values are held.
//!
//! [`gproxy-host-edge`]: https://github.com/LeenHawk/gproxy

pub mod admin;
pub mod console;
pub mod error;
pub mod ingress;
pub mod mount;
pub mod oauth;
pub mod policy;
pub mod portal;
pub mod response;
pub mod session;

pub use error::{ErrorResponse, OAuthEnvelope};
pub use mount::Mount;

use std::sync::Arc;

use axum::{
    Router,
    extract::{DefaultBodyLimit, Path, Request, State},
    response::{IntoResponse, Response},
    routing::get,
};
use gproxy_app::App;
use gproxy_seaorm::BatchConnectionTrait;
use http::{HeaderValue, StatusCode, header};

/// The largest request body this host accepts, before any content-encoding is
/// decoded.
///
/// A gateway forwards whole prompts, and a prompt with images or a long
/// transcript in it is legitimately megabytes; the limit exists to stop an
/// unbounded buffer, not to express a product policy. It applies to the data
/// plane and to the management surfaces alike.
pub const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

/// Everything a handler needs: the instance, and the console bundle.
///
/// Cheap to clone — both fields are `Arc` — which is what lets axum hand a
/// copy to every request.
pub struct HostState<C> {
    app: Arc<App<C>>,
    console: Arc<console::Console>,
}

// Manual, because a derive would demand `C: Clone` for a field that is behind
// an `Arc` either way.
impl<C> Clone for HostState<C> {
    fn clone(&self) -> Self {
        Self {
            app: self.app.clone(),
            console: self.console.clone(),
        }
    }
}

impl<C> HostState<C> {
    /// Wrap an assembled instance. The console is resolved from
    /// [`AppConfig::console`](gproxy_app::config::ConsoleConfig) once here
    /// rather than per request.
    pub fn new(app: Arc<App<C>>) -> Self {
        let console = Arc::new(console::Console::from_config(&app.config().console));
        Self { app, console }
    }

    pub fn app(&self) -> &Arc<App<C>> {
        &self.app
    }

    pub fn console(&self) -> &console::Console {
        &self.console
    }
}

impl<C> std::fmt::Debug for HostState<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostState")
            .field("app", &self.app)
            .finish_non_exhaustive()
    }
}

/// The whole HTTP surface of one instance.
///
/// The fallback is the data plane, which is the opposite of the usual
/// arrangement and deliberate: the gateway's own routes are a short, known
/// list, and everything else is somebody else's API that this instance
/// forwards. A new upstream surface must not require a new route here.
pub fn router<C>(state: HostState<C>) -> Router
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    Router::new()
        .route("/healthz", get(healthz::<C>))
        .route(
            &format!("{}/{{id}}", gproxy_app::publication::PUBLICATION_PATH),
            get(publication::<C>),
        )
        .nest("/admin/api", admin::router(state.clone()))
        .nest("/portal/api", portal::router(state.clone()))
        .fallback(ingress::handle::<C>)
        // Applied after the routes so it covers the fallback too. A body
        // larger than this is refused before it is buffered, which is the
        // point: the limit is a memory bound, not a policy.
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(state)
}

/// Liveness, and the revision this process is serving.
///
/// Deliberately unauthenticated and deliberately thin: it reads the published
/// snapshot's revision and touches neither the database nor the cache, so a
/// load balancer polling it cannot itself become the load that fails it.
async fn healthz<C>(State(state): State<HostState<C>>) -> Response {
    error::ok_json(&serde_json::json!({
        "status": "ok",
        "revision": state.app.snapshot().revision(),
    }))
}

/// A published body, by the random id that is its capability.
///
/// No authentication: the id **is** the credential, exactly as
/// [`App::read_publication`](gproxy_app::App::read_publication) documents.
/// Adding a key check here would break the only thing publication exists for —
/// handing an upstream a URL it can fetch.
async fn publication<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let publication = match state.app.read_publication(&id).await {
        Ok(publication) => publication,
        Err(error) => return ErrorResponse(error).into_response(),
    };
    let mut headers = http::HeaderMap::new();
    if let Some(mime) = publication
        .metadata
        .mime
        .as_deref()
        .and_then(|mime| HeaderValue::from_str(mime).ok())
    {
        headers.insert(header::CONTENT_TYPE, mime);
    }
    if let Some(name) = publication.metadata.filename.as_deref()
        // A filename comes from an upstream and lands in a header, so anything
        // that could break the header or the quoting is refused rather than
        // escaped.
        && !name.contains(['"', '\\', '\r', '\n'])
        && let Ok(value) = HeaderValue::from_str(&format!("inline; filename=\"{name}\""))
    {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    response::passthrough(StatusCode::OK, headers, publication.body)
}

// ----------------------------------------------------------------- shared --

/// Drive one engine call on a thread that owns it.
///
/// **A workaround for a defect below this crate, not a design.** The engine's
/// execution futures are **not `Send`**: `gproxy_core::execute::attempt` holds
/// a `&WireRequest` across its awaits, `WireRequest` carries an `HttpBody`,
/// and `HttpBody::Stream` is `Pin<Box<dyn Stream + Send>>` — `Send` but not
/// `Sync`, so a shared reference to one is not `Send`. The same holds for
/// `gproxy_core::service`. Nothing noticed until now because every existing
/// test runs on `#[tokio::test]`'s current-thread runtime, which imposes no
/// `Send` bound; an axum handler's future does.
///
/// So the non-`Send` future is created and polled to completion on a single
/// blocking-pool thread, and only its result — which *is* `Send`, because a
/// `ByteStream` is — crosses back. `Handle::block_on` enters the main runtime,
/// so anything the call spawns (a connection pool task, a timer) still belongs
/// to it, and the response body that comes back is polled normally afterwards.
///
/// The cost is one blocking-pool thread per in-flight *call*, held until the
/// response head arrives — not for the length of a stream, but still a hard
/// ceiling on concurrency that this gateway must not keep. **The fix is to
/// stop borrowing the request across those awaits in `gproxy-core`**, after
/// which every call site here becomes a plain `.await` and this function goes
/// away.
pub(crate) async fn on_engine_thread<F, Fut, T>(job: F) -> Result<T, gproxy_app::AppError>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = T>,
    T: Send + 'static,
{
    let handle = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || handle.block_on(job()))
        .await
        .map_err(|error| gproxy_app::AppError::internal(format!("engine task: {error}")))
}

/// Wall clock in milliseconds. `gproxy-app`'s own is crate-private, which is
/// correct — a host that needs a clock has one.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// A request id for the operator's log, unique within this process.
///
/// The clock gives it a sortable prefix and the counter makes two requests in
/// the same millisecond distinct. It is not a UUID because nothing joins on it
/// across instances: a request id is read next to the log line that produced
/// it.
pub(crate) fn request_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:x}-{sequence:x}", now_ms())
}

/// The route pattern that matched, for naming an audit action.
///
/// Falls back to the raw path, which only happens when this is called outside
/// a matched route — where the value is a label in a trail row rather than a
/// decision, so a less tidy name is better than none.
pub(crate) fn matched_path(request: &Request) -> &str {
    request
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(axum::extract::MatchedPath::as_str)
        .unwrap_or_else(|| request.uri().path())
}

/// The socket's peer address, or the unspecified address when the server was
/// built without `into_make_service_with_connect_info`.
///
/// The fallback is deliberately **not** loopback. An unknown peer must not be
/// a trusted one: a deployment that forgot the connect-info layer would
/// otherwise start believing `x-forwarded-for` from the open internet.
pub(crate) fn peer_ip(request: &Request) -> std::net::IpAddr {
    request
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|info| info.0.ip())
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
}

/// A JSON body, for the handlers that also need the request's own parts and so
/// cannot take `Json<T>` as an extractor.
///
/// The failure is boxed because it *is* a rendered response — a whole
/// `http::Response` — and an unboxed `Result` would make every caller's `Ok`
/// path carry its size.
pub(crate) async fn json_body<T: serde::de::DeserializeOwned>(
    request: Request,
) -> Result<T, Box<Response>> {
    let body = axum::body::to_bytes(request.into_body(), MAX_BODY_BYTES)
        .await
        .map_err(|_| {
            Box::new((StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response())
        })?;
    serde_json::from_slice(&body).map_err(|error| {
        Box::new(
            ErrorResponse(gproxy_app::AppError::invalid(format!(
                "request body: {error}"
            )))
            .into_response(),
        )
    })
}
