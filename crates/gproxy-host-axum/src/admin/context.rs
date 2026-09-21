//! The two routes of `/admin/api` that precede a scope: `GET /context` and
//! the caller's own session.
//!
//! Everything else on this surface needs to know *what the caller is acting
//! as* before it can answer. These two do not, and must not: a person who
//! administers two organizations has not chosen one yet when the console
//! loads, and signing out cannot depend on having chosen. So they are mounted
//! under their own guard — authenticate and resolve, but do not demand a
//! current scope — and the rest of the surface under the one that does.
//!
//! That is also why the section table marks both of them `Scoped`: they are
//! reachable from every scope, including from not having settled on one.

use axum::{
    Extension, Router,
    extract::{Request, State},
    response::Response,
    routing::get,
};
use gproxy_app::{AdminAdmission, Caller, dto::AdminContextDto};
use gproxy_seaorm::BatchConnectionTrait;
use serde::Serialize;

use crate::HostState;

pub(super) fn routes<C>() -> Router<HostState<C>>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    Router::new()
        .route("/context", get(context::<C>))
        .route("/session", get(session_status).delete(sign_out::<C>))
}

/// What this caller may act as and act on.
///
/// The console renders its navigation from this answer and from nothing else,
/// so the payload is built for that reader: the scopes with their names and
/// the exact header value that selects each, the current one, and the
/// sections it reaches. See [`AdminContextDto`].
async fn context<C>(
    State(state): State<HostState<C>>,
    Extension(caller): Extension<Caller>,
    Extension(admission): Extension<AdminAdmission>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        let data = state.app().data();
        crate::error::ok_json(&AdminContextDto::build(&caller, &admission, &data))
    })
    .await
}

/// Who this request is, as the console renders its header from.
///
/// Kept beside the context route because it answers the same question more
/// cheaply, and every client that already polls it keeps working unchanged.
async fn session_status(Extension(caller): Extension<Caller>) -> Response {
    crate::send(async move {
        crate::error::ok_json(&SessionStatus {
            user_id: caller.user_id,
            user_role: caller.user_role,
            api_key_id: caller.api_key_id,
        })
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionStatus {
    user_id: String,
    user_role: String,
    api_key_id: Option<String>,
}

/// End the session this request arrived on, and clear the cookie.
async fn sign_out<C>(State(state): State<HostState<C>>, request: Request) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { crate::portal::sign_out(state, request).await }).await
}
