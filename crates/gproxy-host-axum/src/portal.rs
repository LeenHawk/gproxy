//! The end user's HTTP surface, `/portal/api`.
//!
//! Same shape as [`crate::admin`] and one difference that matters: the guard
//! requires an authenticated caller and **nothing more**. There is no role
//! check here because there is nothing to check against — every operation on
//! [`Portal`](gproxy_app::Portal) is scoped to the caller it was built from,
//! and none of them takes a user id. Scoping by construction is why this
//! surface is safe for an ordinary account while `/admin/api` is not.
//!
//! The guard puts the [`Caller`] it produced into the request's extensions and
//! the handlers take it as `Extension<Caller>`. A handler routed without the
//! guard therefore fails with a 500 rather than running unauthenticated: the
//! extractor has nothing to produce, and there is no default it could invent.
//!
//! # Sign-in is outside the guard
//!
//! `POST /portal/api/login` runs before there is a caller, so it cannot sit
//! behind a middleware that demands one. `logout` joins it there for a
//! practical reason: a client whose session has already expired still wants
//! its cookie cleared, and refusing that with a 401 leaves a browser holding a
//! cookie it can never get rid of.
//!
//! Sign-in is throttled by client address: this host resolves the address
//! (trusting forwarding headers only from a trusted proxy) and hands it to
//! [`Operations::portal_login_from`], which counts the failures.

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use gproxy_app::{
    AppError, Caller, CallerKind, Operations,
    dto::{ConsentDecision, PortalKeyCreate, PortalPasswordChange, PortalUsageQuery},
};
use gproxy_seaorm::BatchConnectionTrait;
use serde::Deserialize;

use crate::{
    HostState,
    admin::{reply, reply_empty},
    error::ErrorResponse,
    session,
};

/// How many recent requests the portal's own log view returns.
const RECENT_REQUESTS: u64 = 50;

/// Build the request's `Portal` and render one call on it. The snapshot is
/// loaded once and held for the whole expression, as everywhere else.
macro_rules! portal {
    (@empty $state:expr, $caller:expr, $surface:ident.$method:ident($($argument:expr),* $(,)?)) => {{
        let data = $state.app().data();
        let operations = Operations::new($state.app().gproxy(), &data, $state.app().config());
        reply_empty(operations.portal(&$caller).$surface().$method($($argument),*).await)
    }};
    ($state:expr, $caller:expr, $surface:ident.$method:ident($($argument:expr),* $(,)?)) => {{
        let data = $state.app().data();
        let operations = Operations::new($state.app().gproxy(), &data, $state.app().config());
        reply(operations.portal(&$caller).$surface().$method($($argument),*).await)
    }};
    ($state:expr, $caller:expr, $method:ident($($argument:expr),* $(,)?)) => {{
        let data = $state.app().data();
        let operations = Operations::new($state.app().gproxy(), &data, $state.app().config());
        reply(operations.portal(&$caller).$method($($argument),*).await)
    }};
}

/// `/portal/api`, to be nested under that prefix.
pub fn router<C>(state: HostState<C>) -> Router<HostState<C>>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let guarded = Router::new()
        .route("/context", get(context::<C>))
        .route("/models", get(models::<C>))
        .route("/usage", get(usage::<C>))
        .route("/quota", get(quota::<C>))
        .route("/requests", get(recent_requests::<C>))
        .route("/sessions", get(sessions::<C>))
        .route("/keys", get(list_keys::<C>).post(create_key::<C>))
        .route("/keys/{id}", axum::routing::delete(delete_key::<C>))
        .route("/keys/{id}/rotate", post(rotate_key::<C>))
        .route("/keys/{id}/secret", get(reveal_key::<C>))
        .route("/oauth-sessions", get(list_grants::<C>))
        .route(
            "/oauth-sessions/{id}",
            axum::routing::delete(revoke_grant::<C>),
        )
        .route("/password", post(change_password::<C>))
        .route(
            "/oauth/device",
            get(device_details::<C>).post(device_decide::<C>),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            guard::<C>,
        ));

    Router::new()
        .route("/login", post(login::<C>))
        .route("/logout", post(logout::<C>))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            session_audit::<C>,
        ))
        .merge(guarded)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceQuery {
    user_code: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceDecisionBody {
    user_code: String,
    decision: ConsentDecision,
}

/// Device consent is a signed-in person's, as the authorization endpoint's is:
/// a bearer token here would let a token, or a leaked key, approve a device
/// for itself.
fn require_session(caller: &Caller) -> Result<(), AppError> {
    if caller.kind == CallerKind::Session {
        return Ok(());
    }
    Err(AppError::forbidden(
        "approving a device takes a console session, not a token",
    ))
}

/// `GET /portal/api/oauth/device?userCode=`: what the device page shows for
/// the code a person typed, or `NotFound` once it is unknown, decided or
/// expired.
async fn device_details<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
    Query(query): Query<DeviceQuery>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        if let Err(error) = require_session(&caller) {
            return ErrorResponse(error).into_response();
        }
        let data = state.app().data();
        let operations = Operations::new(state.app().gproxy(), &data, state.app().config());
        reply(operations.issuer().device_details(&query.user_code).await)
    })
    .await
}

/// `POST /portal/api/oauth/device`: the person's decision on that code. The
/// device, still polling the token endpoint, learns it on its next poll.
async fn device_decide<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
    Json(body): Json<DeviceDecisionBody>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        if let Err(error) = require_session(&caller) {
            return ErrorResponse(error).into_response();
        }
        let data = state.app().data();
        let operations = Operations::new(state.app().gproxy(), &data, state.app().config());
        reply(
            operations
                .issuer()
                .device_decision(&caller, &body.user_code, body.decision)
                .await,
        )
    })
    .await
}

/// Login and logout are management operations too, including rejected attempts.
async fn session_audit<C: BatchConnectionTrait + Send + Sync + 'static>(
    State(state): State<HostState<C>>,
    request: Request,
    next: Next,
) -> Response {
    crate::send(async move {
        let method = request.method().clone();
        let source = crate::admin::audit_source(&state, &request);
        let action = session::audit_action("portal", crate::matched_path(&request), &method);
        let headers = request.headers().clone();
        let caller = if crate::matched_path(&request).ends_with("/login") {
            None
        } else {
            session::authenticate(state.app(), &method, &headers)
                .await
                .ok()
        };
        let response = next.run(request).await;
        let caller = response.extensions().get::<Caller>().cloned().or(caller);
        crate::admin::audit(state, caller, action, &method, response.status(), source).await;
        response
    })
    .await
}

/// Authenticate, hand the caller to the handler, run, refresh, audit.
///
/// The refresh is [`crate::admin::settle`], and it is here for the same reason
/// it is on the operator's surface: a portal key is minted by an operation
/// that commits and notifies without reloading, and a key its owner cannot
/// use until the next poll is a key that does not work.
async fn guard<C>(State(state): State<HostState<C>>, mut request: Request, next: Next) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        // Copied out because `Next::run` consumes the request and a borrow of one
        // cannot cross an await; see `session::authenticate`.
        let method = request.method().clone();
        let headers = request.headers().clone();
        let action = session::audit_action("portal", crate::matched_path(&request), &method);
        let source_ip = crate::admin::audit_source(&state, &request);
        let caller = match session::authenticate(state.app(), &method, &headers).await {
            Ok(caller) => caller,
            Err(error) => {
                let response = ErrorResponse(error).into_response();
                crate::admin::audit(state, None, action, &method, response.status(), source_ip)
                    .await;
                return response;
            }
        };
        request.extensions_mut().insert(caller.clone());
        let response = next.run(request).await;
        let status = response.status();
        crate::admin::settle(&state, &method).await;
        crate::admin::audit(state, Some(caller), action, &method, status, source_ip).await;
        response
    })
    .await
}

/// Whether a cookie set for this request should carry `Secure`.
///
/// Always, when the settings say so. Otherwise taken from the request's own
/// scheme and — behind a trusted proxy — from `x-forwarded-proto`: derived by
/// default because getting it wrong in the safe direction (marking it `Secure`
/// on a plain-HTTP development instance) makes sign-in silently fail, and
/// switched on by an operator who knows every visitor arrives over HTTPS.
fn secure_cookie<C>(state: &HostState<C>, request: &Request) -> bool {
    if crate::runtime_settings::always_secure_cookie(state.app()) {
        return true;
    }
    let peer = crate::peer_ip(request);
    crate::policy::client_scheme(
        peer,
        request.headers(),
        &crate::runtime_settings::trusted_proxies(state.app()),
    ) == "https"
}

#[derive(Deserialize)]
struct LoginBody {
    name: String,
    password: String,
}

/// Exchange a name and a password for a session cookie.
///
/// The token is returned in the body **as well as** in the cookie: a browser
/// uses the cookie, a script that has no cookie jar uses the body and presents
/// it as a bearer token. Both are the same one-time value; the row keeps only
/// its digest.
async fn login<C>(
    State(state): State<HostState<C>>,
    request: Request,
    // The body is read last, after the parts the cookie decision needs.
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        let secure = secure_cookie(&state, &request);
        let client = crate::policy::client_ip(
            crate::peer_ip(&request),
            request.headers(),
            &crate::runtime_settings::trusted_proxies(state.app()),
        )
        .to_string();
        let body: LoginBody = match crate::json_body(request).await {
            Ok(body) => body,
            Err(response) => return *response,
        };
        let data = state.app().data();
        let operations = Operations::new(state.app().gproxy(), &data, state.app().config());
        let issued = match operations
            .portal_login_from(&body.name, &body.password, &client)
            .await
        {
            Ok(issued) => issued,
            Err(error) => return ErrorResponse(error).into_response(),
        };
        let mut response = crate::error::ok_json(&serde_json::json!({
            "sessionId": issued.id,
            "token": issued.token,
            "expiresAtMs": issued.expires_at_ms,
        }));
        // Attribute sign-in to the newly authenticated account, never to an
        // old cookie carried by a browser switching accounts.
        if let Ok(caller) = state
            .app()
            .authenticator(&data)
            .authenticate_session(&issued.token, crate::now_ms())
            .await
        {
            response.extensions_mut().insert(caller);
        }
        match session::set_cookie(&issued.token, state.app().config().session_ttl_secs, secure) {
            Some(cookie) => session::with_cookie(response, cookie),
            None => response,
        }
    })
    .await
}

async fn logout<C>(State(state): State<HostState<C>>, request: Request) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { sign_out(state, request).await }).await
}

/// The shared body of `POST /portal/api/logout` and
/// `DELETE /admin/api/session`: one session, two front doors.
///
/// The request is taken apart first: a `&Request` cannot cross an await (its
/// body is `Send` but not `Sync`), and nothing here reads a body anyway.
pub(crate) async fn sign_out<C>(state: HostState<C>, request: Request) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        let secure = secure_cookie(&state, &request);
        let (parts, _body) = request.into_parts();
        let token = session::cookie(&parts.headers, session::COOKIE_NAME)
            .map(str::to_owned)
            .or_else(|| gproxy_app::auth::bearer_token(&parts.headers).map(str::to_owned));
        let data = state.app().data();
        let operations = Operations::new(state.app().gproxy(), &data, state.app().config());
        // A token that matches nothing still clears the cookie: the client is
        // ending a session that is already over, and a 401 would leave the browser
        // holding a cookie it can never discard.
        let ended = match token {
            Some(token) => operations.portal_logout(&token).await.unwrap_or(false),
            None => false,
        };
        let response = crate::error::ok_json(&serde_json::json!({ "endedSession": ended }));
        session::with_cookie(response, session::clear_cookie(secure))
    })
    .await
}

async fn context<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { portal!(state, caller, context()) }).await
}

async fn models<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        let data = state.app().data();
        let operations = Operations::new(state.app().gproxy(), &data, state.app().config());
        reply(operations.portal(&caller).models())
    })
    .await
}

async fn usage<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
    Query(query): Query<PortalUsageQuery>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { portal!(state, caller, usage(query)) }).await
}

async fn quota<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { portal!(state, caller, quota()) }).await
}

async fn recent_requests<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { portal!(state, caller, recent_requests(RECENT_REQUESTS)) }).await
}

async fn sessions<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { portal!(state, caller, sessions()) }).await
}

async fn list_keys<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { portal!(state, caller, keys.list()) }).await
}

async fn create_key<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
    Json(write): Json<PortalKeyCreate>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { portal!(state, caller, keys.create(write)) }).await
}

async fn delete_key<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { portal!(@empty state, caller, keys.delete(&id)) }).await
}

async fn rotate_key<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { portal!(state, caller, keys.rotate(&id)) }).await
}

async fn reveal_key<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { portal!(state, caller, keys.reveal(&id)) }).await
}

async fn list_grants<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { portal!(state, caller, oauth_sessions.list()) }).await
}

async fn revoke_grant<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { portal!(@empty state, caller, oauth_sessions.revoke(&id)) }).await
}

async fn change_password<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
    Json(change): Json<PortalPasswordChange>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { portal!(@empty state, caller, password.change(change)) }).await
}
