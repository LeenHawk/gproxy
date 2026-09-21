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
//! **Sign-in is not rate limited.** `gproxy-app` explains why it cannot do it
//! (it has no client address); this host does not yet do it either. It is the
//! one open item in the crate README.

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use gproxy_app::{
    Caller, Operations,
    dto::{PortalKeyCreate, PortalPasswordChange, PortalUsageQuery},
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
        .route_layer(axum::middleware::from_fn_with_state(state, guard::<C>));

    Router::new()
        .route("/login", post(login::<C>))
        .route("/logout", post(logout::<C>))
        .merge(guarded)
}

/// Authenticate, hand the caller to the handler, run, audit.
async fn guard<C>(State(state): State<HostState<C>>, mut request: Request, next: Next) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    // Copied out because `Next::run` consumes the request and a borrow of one
    // cannot cross an await; see `session::authenticate`.
    let method = request.method().clone();
    let headers = request.headers().clone();
    let action = session::audit_action("portal", crate::matched_path(&request), &method);
    let caller = match session::authenticate(state.app(), &method, &headers).await {
        Ok(caller) => caller,
        Err(error) => return ErrorResponse(error).into_response(),
    };
    request.extensions_mut().insert(caller.clone());
    let response = next.run(request).await;
    let status = response.status();
    crate::admin::audit(state, caller, action, &method, status).await;
    response
}

/// Whether a cookie set for this request should carry `Secure`.
///
/// Taken from the request's own scheme and — behind a trusted proxy — from
/// `x-forwarded-proto`. Derived rather than configured because getting it
/// wrong in the safe direction (marking it `Secure` on a plain-HTTP
/// development instance) makes sign-in silently fail.
fn secure_cookie<C>(state: &HostState<C>, request: &Request) -> bool {
    let peer = crate::peer_ip(request);
    crate::policy::client_scheme(
        peer,
        request.headers(),
        &state.app().config().trusted_proxies,
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
    let secure = secure_cookie(&state, &request);
    let body: LoginBody = match crate::json_body(request).await {
        Ok(body) => body,
        Err(response) => return *response,
    };
    let data = state.app().data();
    let operations = Operations::new(state.app().gproxy(), &data, state.app().config());
    let issued = match operations.portal_login(&body.name, &body.password).await {
        Ok(issued) => issued,
        Err(error) => return ErrorResponse(error).into_response(),
    };
    let response = crate::error::ok_json(&serde_json::json!({
        "sessionId": issued.id,
        "token": issued.token,
        "expiresAtMs": issued.expires_at_ms,
    }));
    match session::set_cookie(&issued.token, state.app().config().session_ttl_secs, secure) {
        Some(cookie) => session::with_cookie(response, cookie),
        None => response,
    }
}

async fn logout<C>(State(state): State<HostState<C>>, request: Request) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    sign_out(state, request).await
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
}

async fn context<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    portal!(state, caller, context())
}

async fn models<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let data = state.app().data();
    let operations = Operations::new(state.app().gproxy(), &data, state.app().config());
    reply(operations.portal(&caller).models())
}

async fn usage<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
    Query(query): Query<PortalUsageQuery>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    portal!(state, caller, usage(query))
}

async fn quota<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    portal!(state, caller, quota())
}

async fn recent_requests<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    portal!(state, caller, recent_requests(RECENT_REQUESTS))
}

async fn sessions<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    portal!(state, caller, sessions())
}

async fn list_keys<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    portal!(state, caller, keys.list())
}

async fn create_key<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
    Json(write): Json<PortalKeyCreate>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    portal!(state, caller, keys.create(write))
}

async fn delete_key<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    portal!(@empty state, caller, keys.delete(&id))
}

async fn rotate_key<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    portal!(state, caller, keys.rotate(&id))
}

async fn reveal_key<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    portal!(state, caller, keys.reveal(&id))
}

async fn list_grants<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    portal!(state, caller, oauth_sessions.list())
}

async fn revoke_grant<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    portal!(@empty state, caller, oauth_sessions.revoke(&id))
}

async fn change_password<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
    Json(change): Json<PortalPasswordChange>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    portal!(@empty state, caller, password.change(change))
}
