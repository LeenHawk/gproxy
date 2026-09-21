//! The operator's HTTP surface, `/admin/api`.
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
//! [`Operations`](gproxy_app::Operations) over the request's pinned snapshot,
//! calls exactly one method on it and renders the result. Nothing here
//! validates, authorizes a row, or decides a status.
//!
//! # The middleware, in order
//!
//! 1. **authenticate** — session cookie first, bearer token second;
//! 2. **instance admin** — this whole surface is the operator's, and
//!    [`Operations`] performs no authorization of its own, so the check has to
//!    be here. Organization- and team-scoped administration is the portal's;
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
    Extension, Json, Router,
    extract::{Path, Query, Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use gproxy_app::{
    AppError, Caller, Operations,
    audit::AuditEntry,
    dto::{
        ApiKeyPatch, ApiKeyWrite, AuditQuery, ListQuery, MemberWrite, OAuthClientPatch,
        OAuthClientWrite, OrganizationPatch, OrganizationWrite, PermissionPatch, PermissionWrite,
        PlanLimitPatch, PlanLimitWrite, PlanPatch, PlanWrite, PoolMemberPatch, PoolMemberWrite,
        PoolPatch, PoolWrite, RateLimitPatch, RateLimitWrite, SubscriptionPatch, SubscriptionWrite,
        TeamPatch, TeamWrite, UserPatch, UserWrite,
    },
};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::identity::audit_event;
use http::{Method, StatusCode};
use serde::{Deserialize, Serialize};

use crate::{HostState, error::ErrorResponse, session};

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

/// `/admin/api`, to be nested under that prefix.
///
/// The state is taken by value because the guard below is a
/// `from_fn_with_state` middleware, which needs the value at build time rather
/// than the `Router<S>` placeholder.
pub fn router<C>(state: HostState<C>) -> Router<HostState<C>>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let router = Router::new().route("/session", get(session_status).delete(sign_out::<C>));
    let router = family!(router, "/users", users, UserWrite, UserPatch)
        .route(
            "/users/{id}/password",
            post(set_password::<C>).delete(clear_password::<C>),
        )
        .route(
            "/users/{id}/allowlist",
            axum::routing::put(set_allowlist::<C>),
        )
        .route(
            "/users/{id}/sessions",
            get(user_sessions::<C>).delete(revoke_user_sessions::<C>),
        );
    let router = family!(router, "/api-keys", api_keys, ApiKeyWrite, ApiKeyPatch)
        .route("/api-keys/{id}/rotate", post(rotate_key::<C>))
        .route("/api-keys/{id}/secret", get(reveal_key::<C>));
    let router = family!(
        router,
        "/organizations",
        organizations,
        OrganizationWrite,
        OrganizationPatch
    );
    let router = family!(router, "/teams", teams, TeamWrite, TeamPatch);
    let router = family!(
        router,
        "/permissions",
        permissions,
        PermissionWrite,
        PermissionPatch
    );
    let router = family!(
        router,
        "/rate-limits",
        rate_limits,
        RateLimitWrite,
        RateLimitPatch
    );
    let router = family!(
        router,
        "/subscriptions",
        subscriptions,
        SubscriptionWrite,
        SubscriptionPatch
    );
    let router = family!(router, "/pools", pools, PoolWrite, PoolPatch);
    let router = family!(
        router,
        "/pool-members",
        pool_members,
        PoolMemberWrite,
        PoolMemberPatch
    );
    let router = family!(router, "/plans", plans, PlanWrite, PlanPatch);
    let router = family!(
        router,
        "/plan-limits",
        plan_limits,
        PlanLimitWrite,
        PlanLimitPatch
    );
    // An OAuth client is retired, not deleted: the grants it issued still
    // name it, so the row survives and stops being usable.
    let router = router
        .route(
            "/oauth-clients",
            collection!(oauth_clients, OAuthClientWrite),
        )
        .route(
            "/oauth-clients/{id}",
            item!(oauth_clients, OAuthClientPatch),
        )
        .route("/oauth-clients/{id}/retire", post(retire_client::<C>));

    router
        // Membership is a composite key, so it is addressed by both halves
        // rather than by an id and does not take the family shape above.
        .route(
            "/organizations/{id}/members",
            get(list_members::<C>).post(add_member::<C>),
        )
        .route(
            "/organizations/{id}/members/{user_id}",
            get(get_member::<C>)
                .put(set_member_role::<C>)
                .delete(remove_member::<C>),
        )
        .route(
            "/teams/{id}/members",
            get(list_team_members::<C>).post(add_team_member::<C>),
        )
        .route(
            "/teams/{id}/members/{user_id}",
            get(get_team_member::<C>)
                .put(set_team_member_role::<C>)
                .delete(remove_team_member::<C>),
        )
        .route("/sessions", get(list_sessions::<C>))
        .route("/sessions/{id}", axum::routing::delete(revoke_session::<C>))
        .route("/audit", get(list_audit::<C>))
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
/// page load, and a trail that is 95% `list` is a trail nobody reads. The
/// outcome comes from the status because the body has already been rendered —
/// by design, since an audit write must never be able to change the answer the
/// caller gets.
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

#[derive(Deserialize)]
struct PasswordBody {
    password: String,
}

#[derive(Deserialize)]
struct AllowlistBody {
    /// `null` clears the per-user allowlist and falls back to the global one.
    clients: Option<Vec<String>>,
}

async fn set_password<C>(
    State(state): State<HostState<C>>,
    Path(id): Path<String>,
    Json(body): Json<PasswordBody>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    operations!(state, users.set_password(&id, &body.password))
}

async fn clear_password<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    operations!(state, users.clear_password(&id))
}

async fn set_allowlist<C>(
    State(state): State<HostState<C>>,
    Path(id): Path<String>,
    Json(body): Json<AllowlistBody>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    operations!(state, users.set_allowlist(&id, body.clients))
}

async fn rotate_key<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    operations!(state, api_keys.rotate(&id))
}

async fn reveal_key<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    operations!(state, api_keys.reveal(&id))
}

async fn retire_client<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    operations!(state, oauth_clients.retire(&id))
}

async fn list_members<C>(
    State(state): State<HostState<C>>,
    Path(id): Path<String>,
    Query(mut query): Query<ListQuery>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    // The path names the organization; a query parameter must not be able to
    // point the list at a different one.
    query.organization_id = Some(id);
    operations!(state, members.list(query))
}

async fn get_member<C>(
    State(state): State<HostState<C>>,
    Path((id, user_id)): Path<(String, String)>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    operations!(state, members.get(&id, &user_id))
}

async fn add_member<C>(
    State(state): State<HostState<C>>,
    Path(id): Path<String>,
    Json(write): Json<MemberWrite>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    operations!(state, members.add(&id, write))
}

async fn set_member_role<C>(
    State(state): State<HostState<C>>,
    Path((id, user_id)): Path<(String, String)>,
    Json(patch): Json<gproxy_app::dto::MemberPatch>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    operations!(state, members.set_role(&id, &user_id, patch))
}

async fn remove_member<C>(
    State(state): State<HostState<C>>,
    Path((id, user_id)): Path<(String, String)>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    operations!(@empty state, members.remove(&id, &user_id))
}

async fn list_team_members<C>(
    State(state): State<HostState<C>>,
    Path(id): Path<String>,
    Query(mut query): Query<ListQuery>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    query.team_id = Some(id);
    operations!(state, team_members.list(query))
}

async fn get_team_member<C>(
    State(state): State<HostState<C>>,
    Path((id, user_id)): Path<(String, String)>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    operations!(state, team_members.get(&id, &user_id))
}

async fn add_team_member<C>(
    State(state): State<HostState<C>>,
    Path(id): Path<String>,
    Json(write): Json<MemberWrite>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    operations!(state, team_members.add(&id, write))
}

async fn set_team_member_role<C>(
    State(state): State<HostState<C>>,
    Path((id, user_id)): Path<(String, String)>,
    Json(patch): Json<gproxy_app::dto::MemberPatch>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    operations!(state, team_members.set_role(&id, &user_id, patch))
}

async fn remove_team_member<C>(
    State(state): State<HostState<C>>,
    Path((id, user_id)): Path<(String, String)>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    operations!(@empty state, team_members.remove(&id, &user_id))
}

async fn list_sessions<C>(
    State(state): State<HostState<C>>,
    Query(query): Query<ListQuery>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    operations!(state, sessions.page(query))
}

async fn revoke_session<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    operations!(@empty state, sessions.revoke(&id))
}

async fn user_sessions<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    operations!(state, sessions.list(&id))
}

async fn revoke_user_sessions<C>(
    State(state): State<HostState<C>>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let data = state.app().data();
    let operations = Operations::new(state.app().gproxy(), &data, state.app().config());
    match operations.sessions().revoke_all(&id).await {
        Ok(count) => crate::error::ok_json(&serde_json::json!({ "revoked": count })),
        Err(error) => ErrorResponse(error).into_response(),
    }
}

async fn list_audit<C>(
    State(state): State<HostState<C>>,
    Query(query): Query<AuditQuery>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    operations!(state, audit.query(query))
}

/// Who this request is, as the console renders its header from.
///
/// On the admin surface rather than the portal's because reaching it at all
/// proves the middleware admitted an instance administrator.
async fn session_status(Extension(caller): Extension<Caller>) -> Response {
    crate::error::ok_json(&SessionStatus {
        user_id: caller.user_id,
        user_role: caller.user_role,
        api_key_id: caller.api_key_id,
    })
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
    crate::portal::sign_out(state, request).await
}
