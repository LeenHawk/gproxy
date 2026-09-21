//! The product layer's half of `/admin/api`: the identity families, through
//! [`Operations`](gproxy_app::Operations).
//!
//! The surface's rules — one explicit route per operation, thin handlers, the
//! middleware order and why the audit action is derived rather than typed —
//! are documented once on the parent module, which also owns the macros used
//! here. This file is the route table and nothing else.

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, Request, State},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use gproxy_app::{
    Caller, Operations,
    dto::{
        ApiKeyPatch, ApiKeyWrite, AuditQuery, ListQuery, MemberWrite, OAuthClientPatch,
        OAuthClientWrite, OrganizationPatch, OrganizationWrite, PermissionPatch, PermissionWrite,
        PlanLimitPatch, PlanLimitWrite, PlanPatch, PlanWrite, PoolMemberPatch, PoolMemberWrite,
        PoolPatch, PoolWrite, RateLimitPatch, RateLimitWrite, SubscriptionPatch, SubscriptionWrite,
        TeamPatch, TeamWrite, UserPatch, UserWrite,
    },
};
use gproxy_seaorm::BatchConnectionTrait;
use serde::{Deserialize, Serialize};

use super::{reply, reply_empty};
use crate::{HostState, error::ErrorResponse};

/// The identity routes, to be merged into `/admin/api` and guarded there.
pub(super) fn routes<C>() -> Router<HostState<C>>
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
    crate::send(async move { operations!(state, users.set_password(&id, &body.password)) }).await
}

async fn clear_password<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, users.clear_password(&id)) }).await
}

async fn set_allowlist<C>(
    State(state): State<HostState<C>>,
    Path(id): Path<String>,
    Json(body): Json<AllowlistBody>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, users.set_allowlist(&id, body.clients)) }).await
}

async fn rotate_key<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, api_keys.rotate(&id)) }).await
}

async fn reveal_key<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, api_keys.reveal(&id)) }).await
}

async fn retire_client<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, oauth_clients.retire(&id)) }).await
}

async fn list_members<C>(
    State(state): State<HostState<C>>,
    Path(id): Path<String>,
    Query(mut query): Query<ListQuery>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        // The path names the organization; a query parameter must not be able to
        // point the list at a different one.
        query.organization_id = Some(id);
        operations!(state, members.list(query))
    })
    .await
}

async fn get_member<C>(
    State(state): State<HostState<C>>,
    Path((id, user_id)): Path<(String, String)>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, members.get(&id, &user_id)) }).await
}

async fn add_member<C>(
    State(state): State<HostState<C>>,
    Path(id): Path<String>,
    Json(write): Json<MemberWrite>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, members.add(&id, write)) }).await
}

async fn set_member_role<C>(
    State(state): State<HostState<C>>,
    Path((id, user_id)): Path<(String, String)>,
    Json(patch): Json<gproxy_app::dto::MemberPatch>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, members.set_role(&id, &user_id, patch)) }).await
}

async fn remove_member<C>(
    State(state): State<HostState<C>>,
    Path((id, user_id)): Path<(String, String)>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(@empty state, members.remove(&id, &user_id)) }).await
}

async fn list_team_members<C>(
    State(state): State<HostState<C>>,
    Path(id): Path<String>,
    Query(mut query): Query<ListQuery>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        query.team_id = Some(id);
        operations!(state, team_members.list(query))
    })
    .await
}

async fn get_team_member<C>(
    State(state): State<HostState<C>>,
    Path((id, user_id)): Path<(String, String)>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, team_members.get(&id, &user_id)) }).await
}

async fn add_team_member<C>(
    State(state): State<HostState<C>>,
    Path(id): Path<String>,
    Json(write): Json<MemberWrite>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, team_members.add(&id, write)) }).await
}

async fn set_team_member_role<C>(
    State(state): State<HostState<C>>,
    Path((id, user_id)): Path<(String, String)>,
    Json(patch): Json<gproxy_app::dto::MemberPatch>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, team_members.set_role(&id, &user_id, patch)) })
        .await
}

async fn remove_team_member<C>(
    State(state): State<HostState<C>>,
    Path((id, user_id)): Path<(String, String)>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(@empty state, team_members.remove(&id, &user_id)) }).await
}

async fn list_sessions<C>(
    State(state): State<HostState<C>>,
    Query(query): Query<ListQuery>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, sessions.page(query)) }).await
}

async fn revoke_session<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(@empty state, sessions.revoke(&id)) }).await
}

async fn user_sessions<C>(State(state): State<HostState<C>>, Path(id): Path<String>) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, sessions.list(&id)) }).await
}

async fn revoke_user_sessions<C>(
    State(state): State<HostState<C>>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        let data = state.app().data();
        let operations = Operations::new(state.app().gproxy(), &data, state.app().config());
        match operations.sessions().revoke_all(&id).await {
            Ok(count) => crate::error::ok_json(&serde_json::json!({ "revoked": count })),
            Err(error) => ErrorResponse(error).into_response(),
        }
    })
    .await
}

async fn list_audit<C>(
    State(state): State<HostState<C>>,
    Query(query): Query<AuditQuery>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, audit.query(query)) }).await
}

/// Who this request is, as the console renders its header from.
///
/// On the admin surface rather than the portal's because reaching it at all
/// proves the middleware admitted an instance administrator.
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
