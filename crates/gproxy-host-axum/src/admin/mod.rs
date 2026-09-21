//! The operator's HTTP surface, `/admin/api`.
//!
//! # Two halves, one surface
//!
//! The routes live in two modules because they call two different tables, not
//! because they are two different APIs. [`identity`] is the product layer's:
//! users, keys, organizations, teams, permissions, rate limits, subscriptions,
//! pools, plans, OAuth clients, sessions and the audit trail, all through
//! [`Operations`](gproxy_app::Operations). [`config`] is the handle's: the
//! providers, credentials, models, routes, rewrite rules, endpoints, quotas,
//! prices, transfer, probes, tokenizer and catalogues that
//! [`Gproxy::manage`](gproxy_sdk::Gproxy::manage) already exposes as an
//! operation table.
//!
//! The second half is called on `manage()` directly rather than through a
//! delegating facade in `gproxy-app`. There is nothing for such a facade to
//! decide — the sdk families *are* the operation table — and a third host
//! (Tauri, edge) binds the same handle its own way rather than a wrapper this
//! host happened to invent.
//!
//! # One explicit route per operation
//!
//! Every route below names its method, its path and the operation it calls.
//! There is no path parser, no dispatch table keyed on a string and no
//! `match` over segments — v3 had one and its failure mode was silent: a route
//! that no arm matched fell through to the next surface and became a 404 that
//! looked like a missing row. Here axum owns the matching, a method that is
//! not routed is a 405 rather than a mystery, and a route with no handler does
//! not compile.
//!
//! The handlers are deliberately thin. Each one builds an
//! [`Operations`](gproxy_app::Operations) or a
//! [`Manage`](gproxy_sdk::manage::Manage) over the request, calls exactly one
//! method on it and renders the result. Nothing here validates, authorizes a
//! row, or decides a status.
//!
//! # The middleware, in order
//!
//! 1. **authenticate** — session cookie first, bearer token second;
//! 2. **instance admin** — this whole surface is the operator's, and neither
//!    [`Operations`] nor the sdk's management families perform any
//!    authorization of their own, so the check has to be here.
//!    Organization- and team-scoped administration is the portal's;
//! 3. **same origin**, for an unsafe method on a cookie caller (inside
//!    [`session::authenticate`]);
//! 4. the operation;
//! 5. **audit**, for every method that is not a read. The action name is
//!    derived from the matched route, so a new route cannot forget to name
//!    itself.
//!
//! It is a `route_layer`, so it applies only to routes that matched: an
//! unknown `/admin/api/*` path is a 404 without ever touching the database.

use axum::{
    Router,
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use gproxy_app::{AppError, Caller, Operations, audit::AuditEntry};
use gproxy_sdk::SdkError;
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::identity::audit_event;
use http::{Method, StatusCode};
use serde::Serialize;

use crate::{HostState, error::ErrorResponse, session};

// ---------------------------------------------------------------------------
// The macros both halves are built from. They live here, next to the
// middleware, because what they encode is the surface's shape: a family has
// exactly these routes and a handler does exactly one thing.
//
// `macro_rules!` is textually scoped, so the `mod` declarations below have to
// come after the definitions — which is also the order a reader wants them in.
// ---------------------------------------------------------------------------

/// The five routes every identity family has, plus whatever else it declares.
///
/// A macro rather than a generic function because the families are separate
/// types with separate DTOs; what they share is the *shape*, and the point is
/// that a family cannot accidentally have four of the five or a different
/// method on one of them.
macro_rules! family {
    ($router:expr, $path:literal, $family:ident, $write:ty, $patch:ty) => {
        $router
            .route($path, collection!($family, $write))
            .route(
                concat!($path, "/{id}"),
                item!($family, $patch).delete(
                    |State(state): State<HostState<C>>, Path(id): Path<String>| async move {
                        operations!(@empty state, $family.delete(&id))
                    },
                ),
            )
    };
}

/// `GET` the page and `POST` a new row.
macro_rules! collection {
    ($family:ident, $write:ty) => {
        get(
            |State(state): State<HostState<C>>, Query(query): Query<ListQuery>| async move {
                operations!(state, $family.list(query))
            },
        )
        .post(
            |State(state): State<HostState<C>>, Json(write): Json<$write>| async move {
                operations!(state, $family.create(write))
            },
        )
    };
}

/// `GET` and `PATCH` one row. Kept apart from the delete so a family that
/// retires rather than deletes — an OAuth client, whose grants still name it —
/// can take these two and declare its own third.
macro_rules! item {
    ($family:ident, $patch:ty) => {
        get(
            |State(state): State<HostState<C>>, Path(id): Path<String>| async move {
                operations!(state, $family.get(&id))
            },
        )
        .patch(
            |State(state): State<HostState<C>>,
             Path(id): Path<String>,
             Json(patch): Json<$patch>| async move {
                operations!(state, $family.update(&id, patch))
            },
        )
    };
}

/// Build the request's `Operations` and render one call on it.
///
/// The snapshot is loaded once and held for the whole expression, which is
/// this crate's half of the one-load-per-request rule: `Operations` borrows
/// it, so it cannot outlive the binding and a second load cannot creep in.
macro_rules! operations {
    (@empty $state:expr, $family:ident.$method:ident($($argument:expr),* $(,)?)) => {{
        let data = $state.app().data();
        let operations =
            Operations::new($state.app().gproxy(), &data, $state.app().config());
        reply_empty(operations.$family().$method($($argument),*).await)
    }};
    ($state:expr, $family:ident.$method:ident($($argument:expr),* $(,)?)) => {{
        let data = $state.app().data();
        let operations =
            Operations::new($state.app().gproxy(), &data, $state.app().config());
        reply(operations.$family().$method($($argument),*).await)
    }};
}

/// Render one call on the handle's management families.
///
/// The accessor chain is a token sequence rather than one identifier because
/// four of the families are reached through a group — `rewrite().rules()`,
/// `pricing().rates()`, `endpoints().operation_rules()` — and a macro that
/// only took an identifier would leave those eight families hand-written and
/// free to drift.
///
/// There is no snapshot to pin here: `Manage` borrows the handle, and every
/// family reads the database at the moment it is called.
macro_rules! manage {
    (@empty $state:expr, $($call:tt)+) => {
        reply_sdk_empty($state.app().gproxy().manage().$($call)+.await)
    };
    ($state:expr, $($call:tt)+) => {
        reply_sdk($state.app().gproxy().manage().$($call)+.await)
    };
}

/// The six routes every configuration family has.
///
/// One more than the identity families, because every sdk family also takes a
/// `batch` — which is what makes a console's "disable these five" one revision
/// instead of five.
macro_rules! config_family {
    ($router:expr, $path:literal, [$($family:tt)+], $write:ty, $patch:ty) => {
        $router
            .route($path, config_collection!([$($family)+], $write))
            .route(
                concat!($path, "/batch"),
                post(
                    |State(state): State<HostState<C>>,
                     Json(items): Json<Vec<BatchItem<$write, $patch>>>| async move {
                        manage!(state, $($family)+.batch(items))
                    },
                ),
            )
            .route(concat!($path, "/{id}"), config_item!([$($family)+], $patch))
    };
}

/// `GET` the page and `POST` a new row.
macro_rules! config_collection {
    ([$($family:tt)+], $write:ty) => {
        get(
            |State(state): State<HostState<C>>, Query(query): Query<ListQuery>| async move {
                manage!(state, $($family)+.list(query))
            },
        )
        .post(
            |State(state): State<HostState<C>>, Json(write): Json<$write>| async move {
                manage!(state, $($family)+.create(write))
            },
        )
    };
}

/// `GET`, `PATCH` and `DELETE` one row. Unlike the identity half these three
/// always travel together: no configuration family retires a row instead of
/// deleting it.
macro_rules! config_item {
    ([$($family:tt)+], $patch:ty) => {
        get(
            |State(state): State<HostState<C>>, Path(id): Path<String>| async move {
                manage!(state, $($family)+.get(&id))
            },
        )
        .patch(
            |State(state): State<HostState<C>>,
             Path(id): Path<String>,
             Json(patch): Json<$patch>| async move {
                manage!(state, $($family)+.update(&id, patch))
            },
        )
        .delete(
            |State(state): State<HostState<C>>, Path(id): Path<String>| async move {
                manage!(@empty state, $($family)+.delete(&id))
            },
        )
    };
}

mod config;
mod identity;

/// `/admin/api`, to be nested under that prefix.
///
/// The state is taken by value because the guard below is a
/// `from_fn_with_state` middleware, which needs the value at build time rather
/// than the `Router<S>` placeholder.
pub fn router<C>(state: HostState<C>) -> Router<HostState<C>>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    identity::routes()
        .merge(config::routes())
        .route_layer(axum::middleware::from_fn_with_state(state, guard::<C>))
}

/// Authenticate, require the instance administrator, run, audit.
async fn guard<C>(State(state): State<HostState<C>>, mut request: Request, next: Next) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    // Copied out because `Next::run` consumes the request and a borrow of one
    // cannot cross an await; see `session::authenticate`.
    let method = request.method().clone();
    let headers = request.headers().clone();
    let action = session::audit_action("admin", crate::matched_path(&request), &method);
    let caller = match session::authenticate(state.app(), &method, &headers).await {
        Ok(caller) => caller,
        Err(error) => return ErrorResponse(error).into_response(),
    };
    if !caller.is_instance_admin() {
        return ErrorResponse(AppError::forbidden(
            "the administration API is for instance administrators",
        ))
        .into_response();
    }
    request.extensions_mut().insert(caller.clone());
    let response = next.run(request).await;
    let status = response.status();
    audit(state, caller, action, &method, status).await;
    response
}

/// Append the trail row for a write.
///
/// Reads are not audited: a management list is what a console renders on every
/// page load, and a trail that is 95% `list` is a trail nobody reads. The one
/// deliberate exception is `POST /credentials/{id}/reveal`, which is a read
/// spelled as a write precisely so that it lands here.
///
/// The outcome comes from the status because the body has already been
/// rendered — by design, since an audit write must never be able to change the
/// answer the caller gets.
///
/// It takes the status rather than the response for a mechanical reason worth
/// writing down: `axum::body::Body` is `Send` but not `Sync`, so holding a
/// `&Response` across this `await` would make the enclosing middleware future
/// non-`Send` and the layer would not compile at all.
pub(crate) async fn audit<C>(
    state: HostState<C>,
    caller: Caller,
    action: String,
    method: &Method,
    status: StatusCode,
) where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return;
    }
    let mut entry = AuditEntry::new(action)
        .by(&caller)
        .detail(serde_json::json!({ "status": status.as_u16() }));
    if !status.is_success() {
        entry.outcome = audit_event::OUTCOME_ERROR.to_owned();
    }
    let data = state.app().data();
    Operations::new(state.app().gproxy(), &data, state.app().config())
        .audit()
        .try_record(entry)
        .await;
}

/// One operation's answer as a response.
pub(crate) fn reply<T: Serialize>(result: Result<T, AppError>) -> Response {
    match result {
        Ok(value) => crate::error::ok_json(&value),
        Err(error) => ErrorResponse(error).into_response(),
    }
}

/// An operation with nothing to return. `204` rather than `{}`, so a client
/// does not wonder what it failed to read out of the body.
pub(crate) fn reply_empty(result: Result<(), AppError>) -> Response {
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => ErrorResponse(error).into_response(),
    }
}

/// The same, for the handle's own failures.
///
/// [`AppError::from`] keeps an [`SdkError`] whole exactly so that
/// [`SdkError::status_code`] stays the authority on what a conflict, a spent
/// budget or an upstream's own refusal is worth on the wire; the envelope and
/// the code then come out of [`error`](crate::error) unchanged. Nothing here
/// re-decides a status.
pub(crate) fn reply_sdk<T: Serialize>(result: Result<T, SdkError>) -> Response {
    reply(result.map_err(AppError::from))
}

pub(crate) fn reply_sdk_empty(result: Result<(), SdkError>) -> Response {
    reply_empty(result.map_err(AppError::from))
}
