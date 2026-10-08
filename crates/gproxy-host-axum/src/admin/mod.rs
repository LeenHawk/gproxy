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
//! host happened to invent. The two exceptions are the families whose rows
//! carry an **owner**: see the scope section below.
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
//! [`Operations`](gproxy_app::Operations), a
//! [`Manage`](gproxy_sdk::manage::Manage) or a
//! [`ScopedManage`](gproxy_app::ScopedManage) over the request, calls exactly
//! one method on it and renders the result. Nothing here validates, authorizes
//! a row, or decides a status.
//!
//! # The scope, and why it is not an `if` in a handler
//!
//! This surface is no longer the instance administrator's alone. A caller
//! arrives with one [`AdminScope`]: the operator gets
//! [`Instance`](AdminScope::Instance), an organization or team administrator
//! gets theirs, and **they call the same routes**. What differs is how many
//! rows come back.
//!
//! Two mechanisms, both structural, neither a per-handler check:
//!
//! 1. **The route table declares its section.** Every macro below takes a
//!    section id as a *mandatory* argument, so a family that declares nothing
//!    does not compile. The minimum scope of a section is declared once, in
//!    `gproxy_app::admin_surface`, and the generated handler gates on
//!    [`require_section`](gproxy_app::require_section) — which refuses a
//!    section it does not recognise, so the table is default closed.
//! 2. **A scope-aware family narrows its own query.** `credentials` and
//!    `quotas` go through `ScopedManage` rather than `manage()`, where the
//!    scope participates in building the query and a row outside it is
//!    `NotFound`. Nothing in this module compares an organization id to
//!    anything.
//!
//! # The middleware, in order
//!
//! 1. **authenticate** — session cookie first, bearer token second;
//! 2. **resolve the scope** — [`AdminScope::resolve`], the one place a scope
//!    is derived, and the one place the `x-gproxy-admin-scope` header is read.
//!    A caller who administers nothing is refused the whole surface here, as
//!    they were before scopes existed;
//! 3. **same origin**, for an unsafe method on a cookie caller (inside
//!    [`session::authenticate`]);
//! 4. the operation, gated on its declared section;
//! 5. **refresh**, for every method that is not a read: an operation commits
//!    and notifies but does not reload, so the host rebuilds the identity
//!    snapshot before it answers. See [`settle`];
//! 6. **audit**, for every management API operation, including reads. The action name is
//!    derived from the matched route, so a new route cannot forget to name
//!    itself.
//!
//! It is a `route_layer`, so it applies only to routes that matched: an
//! unknown `/admin/api/*` path is a 404 without ever touching the database.
//!
//! [`context`] and `/session` are mounted under a lighter guard that stops
//! after step 2 without demanding a *current* scope; see that module.

use axum::{
    Router,
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use gproxy_app::{
    AdminAdmission, AdminScope, AppError, Caller, Operations, SCOPE_HEADER, audit::AuditEntry,
};
use gproxy_sdk::SdkError;
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::identity::audit_event;
use http::{Method, StatusCode};
use serde::Serialize;

use crate::{HostState, error::ErrorResponse, session};

// ---------------------------------------------------------------------------
// The macros both halves are built from. They live here, next to the
// middleware, because what they encode is the surface's shape: a family has
// exactly these routes, it declares which scopes reach it, and a handler does
// exactly one thing.
//
// `macro_rules!` is textually scoped, so the `mod` declarations below have to
// come after the definitions — which is also the order a reader wants them in.
// ---------------------------------------------------------------------------

/// Refuse the request unless `$scope` reaches `$section`.
///
/// The section id is a mandatory argument of every route macro below, so this
/// cannot be skipped by forgetting to write it: the macro would not expand.
/// The minimum scope itself is declared once, in `gproxy_app::admin_surface`,
/// and an id that table does not know is refused — a route naming a section
/// nobody declared is closed rather than open.
macro_rules! gate {
    ($section:literal, $scope:expr) => {
        if let Err(error) = gproxy_app::require_section(&$scope, $section) {
            return ErrorResponse(error).into_response();
        }
    };
}

/// The five routes every identity family has, plus whatever else it declares.
///
/// A macro rather than a generic function because the families are separate
/// types with separate DTOs; what they share is the *shape*, and the point is
/// that a family cannot accidentally have four of the five or a different
/// method on one of them — nor reach a scope it never declared.
macro_rules! family {
    ($router:expr, $path:literal, $family:ident, $write:ty, $patch:ty, $section:literal) => {
        $router
            .route($path, collection!($family, $write, $section))
            .route(
                concat!($path, "/{id}"),
                item!($family, $patch, $section).delete(
                    |State(state): State<HostState<C>>,
                     Extension(scope): Extension<AdminScope>,
                     Path(id): Path<String>| crate::send(async move {
                        operations!(@empty state, scope, $section, $family.delete(&id))
                    }),
                ),
            )
    };
}

/// `GET` the page and `POST` a new row.
macro_rules! collection {
    ($family:ident, $write:ty, $section:literal) => {
        get(
            |State(state): State<HostState<C>>,
             Extension(scope): Extension<AdminScope>,
             Query(query): Query<ListQuery>| {
                crate::send(async move { operations!(state, scope, $section, $family.list(query)) })
            },
        )
        .post(
            |State(state): State<HostState<C>>,
             Extension(scope): Extension<AdminScope>,
             Json(write): Json<$write>| {
                crate::send(
                    async move { operations!(state, scope, $section, $family.create(write)) },
                )
            },
        )
    };
}

/// `GET` and `PATCH` one row. Kept apart from the delete so a family that
/// retires rather than deletes — an OAuth client, whose grants still name it —
/// can take these two and declare its own third.
macro_rules! item {
    ($family:ident, $patch:ty, $section:literal) => {
        get(
            |State(state): State<HostState<C>>,
             Extension(scope): Extension<AdminScope>,
             Path(id): Path<String>| {
                crate::send(async move { operations!(state, scope, $section, $family.get(&id)) })
            },
        )
        .patch(
            |State(state): State<HostState<C>>,
             Extension(scope): Extension<AdminScope>,
             Path(id): Path<String>,
             Json(patch): Json<$patch>| {
                crate::send(async move {
                    operations!(state, scope, $section, $family.update(&id, patch))
                })
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
    (@empty $state:expr, $scope:expr, $section:literal, $family:ident.$method:ident($($argument:expr),* $(,)?)) => {{
        gate!($section, $scope);
        let data = $state.app().data();
        let operations =
            Operations::new($state.app().gproxy(), &data, $state.app().config());
        reply_empty(operations.$family().$method($($argument),*).await)
    }};
    ($state:expr, $scope:expr, $section:literal, $family:ident.$method:ident($($argument:expr),* $(,)?)) => {{
        gate!($section, $scope);
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
    (@empty $state:expr, $scope:expr, $section:literal, $($call:tt)+) => {{
        gate!($section, $scope);
        reply_sdk_empty($state.app().gproxy().manage().$($call)+.await)
    }};
    ($state:expr, $scope:expr, $section:literal, $($call:tt)+) => {{
        gate!($section, $scope);
        reply_sdk($state.app().gproxy().manage().$($call)+.await)
    }};
}

/// Render one call on a **scope-aware** family.
///
/// The same shape as [`manage!`], through `gproxy-app`'s `ScopedManage`, which
/// narrows the query and admits the row. The scope is passed rather than
/// consulted: this macro contains no comparison, and neither does any handler
/// that uses it.
macro_rules! scoped {
    (@empty $state:expr, $scope:expr, $section:literal, $family:ident.$method:ident($($argument:expr),* $(,)?)) => {{
        gate!($section, $scope);
        let data = $state.app().data();
        let scoped = ScopedManage::new($state.app().gproxy(), &data, &$scope);
        reply_empty(scoped.$family().$method($($argument),*).await)
    }};
    ($state:expr, $scope:expr, $section:literal, $family:ident.$method:ident($($argument:expr),* $(,)?)) => {{
        gate!($section, $scope);
        let data = $state.app().data();
        let scoped = ScopedManage::new($state.app().gproxy(), &data, &$scope);
        reply(scoped.$family().$method($($argument),*).await)
    }};
}

/// The six routes every configuration family has.
///
/// One more than the identity families, because every sdk family also takes a
/// `batch` — which is what makes a console's "disable these five" one revision
/// instead of five.
macro_rules! config_family {
    ($router:expr, $path:literal, [$($family:tt)+], $write:ty, $patch:ty, $section:literal) => {
        $router
            .route($path, config_collection!([$($family)+], $write, $section))
            .route(
                concat!($path, "/batch"),
                post(
                    |State(state): State<HostState<C>>,
                     Extension(scope): Extension<AdminScope>,
                     Json(items): Json<Vec<BatchItem<$write, $patch>>>| crate::send(async move {
                        manage!(state, scope, $section, $($family)+.batch(items))
                    }),
                ),
            )
            .route(concat!($path, "/{id}"), config_item!([$($family)+], $patch, $section))
    };
}

/// `GET` the page and `POST` a new row.
macro_rules! config_collection {
    ([$($family:tt)+], $write:ty, $section:literal) => {
        get(
            |State(state): State<HostState<C>>,
             Extension(scope): Extension<AdminScope>,
             Query(query): Query<ListQuery>| crate::send(async move {
                manage!(state, scope, $section, $($family)+.list(query))
            }),
        )
        .post(
            |State(state): State<HostState<C>>,
             Extension(scope): Extension<AdminScope>,
             Json(write): Json<$write>| crate::send(async move {
                manage!(state, scope, $section, $($family)+.create(write))
            }),
        )
    };
}

/// `GET`, `PATCH` and `DELETE` one row. Unlike the identity half these three
/// always travel together: no configuration family retires a row instead of
/// deleting it.
macro_rules! config_item {
    ([$($family:tt)+], $patch:ty, $section:literal) => {
        get(
            |State(state): State<HostState<C>>,
             Extension(scope): Extension<AdminScope>,
             Path(id): Path<String>| crate::send(async move {
                manage!(state, scope, $section, $($family)+.get(&id))
            }),
        )
        .patch(
            |State(state): State<HostState<C>>,
             Extension(scope): Extension<AdminScope>,
             Path(id): Path<String>,
             Json(patch): Json<$patch>| crate::send(async move {
                manage!(state, scope, $section, $($family)+.update(&id, patch))
            }),
        )
        .delete(
            |State(state): State<HostState<C>>,
             Extension(scope): Extension<AdminScope>,
             Path(id): Path<String>| crate::send(async move {
                manage!(@empty state, scope, $section, $($family)+.delete(&id))
            }),
        )
    };
}

/// The same six routes, for a family whose rows carry an owner.
///
/// Identical to [`config_family!`] except that every call goes through
/// `ScopedManage`. There are exactly two of these — `credentials` and
/// `quotas` — and the duplication is deliberate: the two macros are the two
/// answers to "may this caller see this row", and collapsing them into one
/// with a flag would put that decision back into a conditional.
macro_rules! scoped_family {
    ($router:expr, $path:literal, $family:ident, $write:ty, $patch:ty, $section:literal) => {
        $router
            .route(
                $path,
                get(
                    |State(state): State<HostState<C>>,
                     Extension(scope): Extension<AdminScope>,
                     Query(query): Query<ListQuery>| crate::send(async move {
                        scoped!(state, scope, $section, $family.list(query))
                    }),
                )
                .post(
                    |State(state): State<HostState<C>>,
                     Extension(scope): Extension<AdminScope>,
                     Json(write): Json<$write>| crate::send(async move {
                        scoped!(state, scope, $section, $family.create(write))
                    }),
                ),
            )
            .route(
                concat!($path, "/batch"),
                post(
                    |State(state): State<HostState<C>>,
                     Extension(scope): Extension<AdminScope>,
                     Json(items): Json<Vec<BatchItem<$write, $patch>>>| crate::send(async move {
                        scoped!(state, scope, $section, $family.batch(items))
                    }),
                ),
            )
            .route(
                concat!($path, "/{id}"),
                get(
                    |State(state): State<HostState<C>>,
                     Extension(scope): Extension<AdminScope>,
                     Path(id): Path<String>| crate::send(async move {
                        scoped!(state, scope, $section, $family.get(&id))
                    }),
                )
                .patch(
                    |State(state): State<HostState<C>>,
                     Extension(scope): Extension<AdminScope>,
                     Path(id): Path<String>,
                     Json(patch): Json<$patch>| crate::send(async move {
                        scoped!(state, scope, $section, $family.update(&id, patch))
                    }),
                )
                .delete(
                    |State(state): State<HostState<C>>,
                     Extension(scope): Extension<AdminScope>,
                     Path(id): Path<String>| crate::send(async move {
                        scoped!(@empty state, scope, $section, $family.delete(&id))
                    }),
                ),
            )
    };
}

mod config;
mod context;
mod credential_login;
#[cfg(not(target_arch = "wasm32"))]
mod fonts;
mod identity;
mod observation;

/// `/admin/api`, to be nested under that prefix.
///
/// The state is taken by value because the guards below are
/// `from_fn_with_state` middleware, which need the value at build time rather
/// than the `Router<S>` placeholder.
///
/// Two guards, not one. Everything that acts on rows needs a settled scope;
/// `/context` and `/session` precede the choice and must answer without one.
///
/// `/update` joins the scoped half **only when the host supplied an
/// [`UpdateService`](crate::update::UpdateService)**. It is one `if` rather
/// than a `cfg` because the fact it tests is a runtime one: the same compiled
/// router serves the native binary, which owns an executable to replace, and
/// the edge, which does not.
pub fn router<C>(state: HostState<C>) -> Router<HostState<C>>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let mut families = identity::routes()
        .merge(config::routes())
        .merge(observation::routes())
        .merge(credential_login::routes());
    #[cfg(not(target_arch = "wasm32"))]
    if state.console().is_enabled() {
        families = families.merge(fonts::routes::<C>());
    }
    if state.updates().is_some() {
        families = families.merge(crate::update::routes::<C>());
    }
    let scoped = families.route_layer(axum::middleware::from_fn_with_state(
        state.clone(),
        guard::<C>,
    ));
    let unscoped = context::routes().route_layer(axum::middleware::from_fn_with_state(
        state,
        context_guard::<C>,
    ));
    scoped.merge(unscoped)
}

/// Authenticate, resolve the scope, require one, run, audit.
async fn guard<C>(State(state): State<HostState<C>>, mut request: Request, next: Next) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        // Copied out because `Next::run` consumes the request and a borrow of one
        // cannot cross an await; see `session::authenticate`.
        let method = request.method().clone();
        let headers = request.headers().clone();
        let action = session::audit_action("admin", crate::matched_path(&request), &method);
        let source_ip = audit_source(&state, &request);
        let (caller, admission) = match admit(&state, &method, &headers).await {
            Ok(admitted) => admitted,
            Err(error) => {
                let response = ErrorResponse(error).into_response();
                audit(state, None, action, &method, response.status(), source_ip).await;
                return response;
            }
        };
        let scope = match admission.require() {
            Ok(scope) => scope.clone(),
            Err(error) => {
                let response = ErrorResponse(error).into_response();
                audit(
                    state,
                    Some(caller),
                    action,
                    &method,
                    response.status(),
                    source_ip,
                )
                .await;
                return response;
            }
        };
        request.extensions_mut().insert(caller.clone());
        request.extensions_mut().insert(scope);
        let response = next.run(request).await;
        let status = response.status();
        settle(&state, &method).await;
        audit(state, Some(caller), action, &method, status, source_ip).await;
        response
    })
    .await
}

/// The same without the last step: `/context` and `/session` are reachable by
/// a caller who administers several scopes and has named none.
async fn context_guard<C>(
    State(state): State<HostState<C>>,
    mut request: Request,
    next: Next,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        let method = request.method().clone();
        let headers = request.headers().clone();
        let action = session::audit_action("admin", crate::matched_path(&request), &method);
        let source_ip = audit_source(&state, &request);
        let (caller, admission) = match admit(&state, &method, &headers).await {
            Ok(admitted) => admitted,
            Err(error) => {
                let response = ErrorResponse(error).into_response();
                audit(state, None, action, &method, response.status(), source_ip).await;
                return response;
            }
        };
        request.extensions_mut().insert(caller.clone());
        request.extensions_mut().insert(admission);
        let response = next.run(request).await;
        let status = response.status();
        settle(&state, &method).await;
        audit(state, Some(caller), action, &method, status, source_ip).await;
        response
    })
    .await
}

/// Who is calling and what they may act as: the two steps both guards share.
///
/// [`AdminScope::resolve`] is the only place in this workspace that derives a
/// scope from a caller, and this is its only call site.
async fn admit<C>(
    state: &HostState<C>,
    method: &Method,
    headers: &http::HeaderMap,
) -> Result<(Caller, AdminAdmission), AppError>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        let caller = session::authenticate(state.app(), method, headers).await?;
        let data = state.app().data();
        let requested = headers
            .get(SCOPE_HEADER)
            .and_then(|value| value.to_str().ok());
        let admission = AdminScope::resolve(&caller, &data, requested)?;
        Ok((caller, admission))
    })
    .await
}

/// Rebuild the identity snapshot if this request moved the revision — the
/// host's half of `gproxy-app`'s write contract.
///
/// An operation commits its rows and tells the peers; it does not reload,
/// because a `Writer` holds the handle and a snapshot and never the `App` that
/// publishes one. Without this call a key minted through `POST /admin/api/api-keys`
/// is a `401` on the very next request, a permission that was granted is still
/// refused, and a membership that exists reads as "administers nothing" —
/// until the process restarts.
///
/// It is here rather than in the handlers because "this request could have
/// written" is a property of the method, and the method is what the middleware
/// already classifies for the audit trail. It is a durable revision poll
/// rather than a drained notification on purpose: a `MemoryCache` publish
/// reaches this process synchronously, but a Redis publish is a round trip and
/// the subscription learns about it whenever it learns about it — so only the
/// database can answer "did I just write this" in time for the answer to
/// matter. A read costs nothing extra, and a write that failed finds the
/// revision where it left it and rebuilds nothing.
///
/// A failure is a warning, never the caller's problem: the write landed, and
/// the poll or a peer's notification will bring this instance to it.
pub(crate) async fn settle<C>(state: &HostState<C>, method: &Method)
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return;
    }
    crate::send(async move {
        if let Err(error) = state.app().sync_now().await {
            tracing::warn!(%error, "the identity snapshot was not refreshed after a write");
        }
    })
    .await;
}

/// Append the trail row for a write.
///
/// Every non-channel API operation is audited, including reads. Channel model
/// and service requests are observed by the ingress capture path instead.
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
    caller: Option<Caller>,
    action: String,
    method: &Method,
    status: StatusCode,
    source_ip: String,
) where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        let mut entry = AuditEntry::new(action)
            .source_ip(Some(source_ip))
            .detail(serde_json::json!({ "status": status.as_u16(), "method": method.as_str() }));
        if let Some(caller) = caller {
            entry = entry.by(&caller);
        }
        if !status.is_success() {
            entry.outcome = audit_event::OUTCOME_ERROR.to_owned();
        }
        let data = state.app().data();
        Operations::new(state.app().gproxy(), &data, state.app().config())
            .audit()
            .try_record(entry)
            .await;
    })
    .await
}

pub(crate) fn audit_source<C: BatchConnectionTrait + Send + Sync + 'static>(
    state: &HostState<C>,
    request: &Request,
) -> String {
    crate::policy::client_ip(
        crate::peer_ip(request),
        request.headers(),
        &crate::runtime_settings::trusted_proxies(state.app()),
    )
    .to_string()
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
