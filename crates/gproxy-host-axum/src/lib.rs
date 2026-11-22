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
//! # The route table
//!
//! | Route | What it is |
//! |---|---|
//! | `GET /healthz` | liveness and the published revision |
//! | `GET /publications/{id}` | a published body, by its capability id |
//! | `/admin/api/*` | the operator surface, one `MethodRouter` per operation |
//! | `/admin/api/update` | self-update, **only when the host supplied an [`UpdateService`]** |
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
//! A websocket is the same rule with a longer clock. [`websocket`] moves the
//! same [`response::Trailer`] into the duplex pump, so a realtime session that
//! runs for an hour holds its concurrency permit for that hour and gives it
//! back when the socket closes — not when the `101` was written.
//!
//! # A client that goes away cancels the upstream call
//!
//! The same values, for the same reason. A client never says it is leaving: the
//! departure is a *drop* — of the handler future before a head was written, of
//! the response body mid-stream — so the request's cancellation token is owned
//! by a guard ([`response::CancelOnDrop`]) that fires on `Drop`, and the guard
//! travels with the rest of the request's state into [`response::LeasedBody`].
//! A socket cancels explicitly instead, because its pump is a task hyper spawned
//! and there is no handler future left to drop.
//!
//! The guard is disarmed the moment the response reaches its own end. Core reads
//! the token when it decides between `Completed` and `Cancelled`, so a token
//! fired after the last byte would record a finished request as an abandoned
//! one. [`response::CancelOnDrop`] has the whole rule.

pub mod admin;
pub mod console;
#[cfg(feature = "documentation")]
pub mod documentation;
pub mod error;
pub mod ingress;
mod memo;
pub mod mount;
pub mod oauth;
pub mod policy;
pub mod portal;
pub mod response;
pub mod runtime_settings;
pub mod serve;
pub mod session;
pub mod update;
pub mod websocket;

pub use error::{ErrorResponse, OAuthEnvelope};
pub use mount::Mount;
pub use update::{
    Announcement, AnnouncementContent, AnnouncementSeverity, AppliedUpdate, UpdateFailure,
    UpdateProgress, UpdateReport, UpdateSchedule, UpdateService,
};

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

/// The largest body a management route accepts.
///
/// Management requests are authenticated before their bodies are read, so
/// this is a memory bound rather than a defence, and it is sized for the
/// largest of them: a configuration import carrying a whole instance. The data
/// plane does not use it; its caps are settings, read per request in
/// `ingress`.
pub const MAX_BODY_BYTES: usize = 50 * 1024 * 1024;

/// The largest sign-in body. A name and a password, read before there is any
/// caller to hold responsible for a larger one.
pub const SIGN_IN_BODY_BYTES: usize = 64 * 1024;

/// Everything a handler needs: the instance, the console bundle, and whatever
/// the host could supply that this crate cannot decide for itself.
///
/// Cheap to clone — every field is an `Arc` or an `Option` of one — which is
/// what lets axum hand a copy to every request.
pub struct HostState<C> {
    app: Arc<App<C>>,
    console: Arc<console::Console>,
    /// The self-update implementation, when the host owns an executable to
    /// replace. `None` otherwise, and the routes are then never mounted. See
    /// [`update`].
    updates: Option<Arc<dyn update::UpdateService>>,
    derived: Arc<Derived>,
}

/// What requests read from the published snapshots in a shape of their own,
/// kept for as long as the snapshot it came from is current.
#[derive(Default)]
struct Derived {
    policy: memo::Memo<Arc<gproxy_app::AppData>, runtime_settings::PolicyLists>,
    mounts:
        memo::Memo<(Arc<gproxy_sdk::RoutingTable>, Arc<gproxy_core::CoreData>), mount::MountIndex>,
}

// Manual, because a derive would demand `C: Clone` for a field that is behind
// an `Arc` either way.
impl<C> Clone for HostState<C> {
    fn clone(&self) -> Self {
        Self {
            app: self.app.clone(),
            console: self.console.clone(),
            updates: self.updates.clone(),
            derived: self.derived.clone(),
        }
    }
}

impl<C> HostState<C> {
    /// Wrap an assembled instance. The console is resolved from
    /// [`AppConfig::console`](gproxy_app::config::ConsoleConfig) once here
    /// rather than per request.
    pub fn new(app: Arc<App<C>>) -> Self {
        let console = {
            let data = std::path::Path::new(app.config().data_dir.as_deref().unwrap_or("data"));
            Arc::new(
                console::Console::from_config(&app.config().console)
                    .with_font_cache(data.join("fonts"))
                    .with_custom_bundle(data.join("console")),
            )
        };
        Self {
            app,
            console,
            updates: None,
            derived: Arc::default(),
        }
    }

    /// Supply a self-update implementation, which mounts
    /// `/admin/api/update`.
    ///
    /// Only a host that owns its own executable calls this. Everything the
    /// routes do is the trait's; this crate neither downloads nor writes
    /// anything. See [`update`] for why that separation is structural rather
    /// than tidy.
    pub fn with_updates(mut self, service: Arc<dyn update::UpdateService>) -> Self {
        self.updates = Some(service);
        self
    }

    pub fn app(&self) -> &Arc<App<C>> {
        &self.app
    }

    /// The CORS origins and trusted proxies of the current settings, parsed
    /// once per published snapshot rather than on every request.
    pub(crate) fn policy_lists(&self) -> Arc<runtime_settings::PolicyLists> {
        let data = self.app.data();
        self.derived.policy.get(&data, || {
            runtime_settings::PolicyLists::of(&self.app, &data)
        })
    }

    /// The mount names of these two snapshots, built once for each pair.
    pub(crate) fn mount_index(
        &self,
        routing: &Arc<gproxy_sdk::RoutingTable>,
        core: &Arc<gproxy_core::CoreData>,
    ) -> Arc<mount::MountIndex> {
        let key = (routing.clone(), core.clone());
        self.derived
            .mounts
            .get(&key, || mount::MountIndex::build(routing, core))
    }

    /// The host's self-update implementation, if it supplied one. `None` is
    /// what keeps the update routes off a router whose host has no executable
    /// to replace.
    pub fn updates(&self) -> Option<&Arc<dyn update::UpdateService>> {
        self.updates.as_ref()
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
        .route(
            "/robots.txt",
            get(|| async { "User-agent: *\nDisallow: /\n" }),
        )
        .route("/healthz", get(healthz::<C>))
        .route("/info", get(runtime_settings::info::<C>))
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
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            runtime_settings::cors::<C>,
        ))
        .layer(axum::middleware::map_response(
            |mut response: Response| async move {
                response.headers_mut().insert(
                    "x-robots-tag",
                    HeaderValue::from_static("noindex, nofollow"),
                );
                response
            },
        ))
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
        // A filename comes from an upstream and lands in a header, so
        // anything that could break the header or the quoting is refused
        // rather than escaped.
        && !name.contains(['"', '\\', '\r', '\n'])
        && let Ok(value) = HeaderValue::from_str(&format!("inline; filename=\"{name}\""))
    {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    // The type came from an upstream and the body is served inline on the
    // console's origin. Taken at its word — no sniffing — and sandboxed,
    // so an HTML or SVG "image" runs no script with the console's origin.
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("sandbox; default-src 'none'; style-src 'unsafe-inline'"),
    );
    response::passthrough(StatusCode::OK, headers, publication.body)
}

// ----------------------------------------------------------------- shared --

/// Wall clock in milliseconds. `gproxy-app`'s own is crate-private, which is
/// correct — a host that needs a clock has one.
pub(crate) fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::SystemTime::UNIX_EPOCH)
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
    // A JSON media type, not merely a JSON-shaped body: a cross-site HTML form
    // can post `text/plain` whose body parses as JSON, and nothing else about
    // a sign-in would tell that apart from the console's own `fetch`.
    let is_json = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(|mime| mime.trim().to_ascii_lowercase())
        .is_some_and(|mime| mime == "application/json" || mime.ends_with("+json"));
    if !is_json {
        return Err(Box::new(
            (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "expected an application/json body",
            )
                .into_response(),
        ));
    }
    let body = axum::body::to_bytes(request.into_body(), SIGN_IN_BODY_BYTES)
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

#[cfg(feature = "documentation")]
pub(crate) fn document_system(api: &mut documentation::Api) {
    use serde_json::json;
    api.route(
        "get",
        "/healthz",
        "",
        None,
        vec![],
        Some(json!({
            "type":"object", "required":["status","revision"],
            "properties":{"status":{"const":"ok"},"revision":{"type":"integer"}}
        })),
    )["security"] = json!([]);
    api.route("get", "/info", "", None, vec![], Some(json!({
        "type":"object", "required":["instanceName","footerText","footerUrl","version","hash","fontManagement"],
        "properties":{"instanceName":{"type":"string"},
            "footerText":{"type":["string","null"]},"footerUrl":{"type":["string","null"]},
            "version":{"type":"string"},"hash":{"type":"string"},"fontManagement":{"type":"boolean"}}
    })))["security"] = json!([]);
    for (path, media, schema) in [
        (
            "/robots.txt".to_owned(),
            "text/plain",
            json!({"type":"string"}),
        ),
        (
            format!("{}/{{id}}", gproxy_app::publication::PUBLICATION_PATH),
            "*/*",
            json!({"type":"string","format":"binary"}),
        ),
    ] {
        let operation = api.route("get", &path, "", None, vec![], None);
        operation["security"] = json!([]);
        operation["responses"]
            .as_object_mut()
            .unwrap()
            .remove("204");
        operation["responses"]["200"] =
            json!({"description":"Success","content":{media:{"schema":schema}}});
    }
}
