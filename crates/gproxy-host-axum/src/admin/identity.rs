//! The product layer's half of `/admin/api`: the identity families, through
//! [`Operations`](gproxy_app::Operations).
//!
//! The surface's rules — one explicit route per operation, thin handlers, the
//! middleware order, how a section declares which scopes reach it, and why the
//! audit action is derived rather than typed — are documented once on the
//! parent module, which also owns the macros used here. This file is the route
//! table and nothing else.
//!
//! **Every family here is instance machinery today.** Organization-scoped
//! identity — an organization administrator managing their own members, teams
//! and keys — is the next step of the scope model and is not implemented; the
//! sections are declared `Instance`, so an organization or team scope reaches
//! none of them and the surface is correct rather than half-open. See
//! `gproxy_app::admin_surface`.

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use gproxy_app::{
    AdminScope, Operations,
    dto::{
        ApiKeyPatch, ApiKeyWrite, AuditQuery, ListQuery, MemberWrite, OAuthClientPatch,
        OAuthClientWrite, OrganizationPatch, OrganizationWrite, PermissionPatch, PermissionWrite,
        RateLimitPatch, RateLimitWrite, TeamPatch, TeamWrite, UserPatch, UserWrite,
    },
};
use gproxy_seaorm::BatchConnectionTrait;
use serde::Deserialize;

use super::{reply, reply_empty};
use crate::{HostState, error::ErrorResponse};

/// The identity routes, to be merged into `/admin/api` and guarded there.
pub(super) fn routes<C>() -> Router<HostState<C>>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let router = family!(
        Router::new(),
        "/users",
        users,
        UserWrite,
        UserPatch,
        "users"
    )
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
    let router = family!(
        router,
        "/api-keys",
        api_keys,
        ApiKeyWrite,
        ApiKeyPatch,
        "api-keys"
    )
    .route("/api-keys/{id}/rotate", post(rotate_key::<C>))
    .route("/api-keys/{id}/secret", get(reveal_key::<C>));
    let router = family!(
        router,
        "/organizations",
        organizations,
        OrganizationWrite,
        OrganizationPatch,
        "organizations"
    );
    let router = family!(router, "/teams", teams, TeamWrite, TeamPatch, "teams");
    let router = family!(
        router,
        "/permissions",
        permissions,
        PermissionWrite,
        PermissionPatch,
        "permissions"
    );
    let router = family!(
        router,
        "/rate-limits",
        rate_limits,
        RateLimitWrite,
        RateLimitPatch,
        "rate-limits"
    );
    // An OAuth client is retired, not deleted: the grants it issued still
    // name it, so the row survives and stops being usable.
    let router = router
        .route(
            "/oauth-clients",
            collection!(oauth_clients, OAuthClientWrite, "oauth-clients"),
        )
        .route(
            "/oauth-clients/{id}",
            item!(oauth_clients, OAuthClientPatch, "oauth-clients"),
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
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
    Json(body): Json<PasswordBody>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        operations!(
            state,
            scope,
            "users",
            users.set_password(&id, &body.password)
        )
    })
    .await
}

async fn clear_password<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, scope, "users", users.clear_password(&id)) }).await
}

async fn set_allowlist<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
    Json(body): Json<AllowlistBody>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        operations!(
            state,
            scope,
            "users",
            users.set_allowlist(&id, body.clients)
        )
    })
    .await
}

async fn rotate_key<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, scope, "api-keys", api_keys.rotate(&id)) }).await
}

async fn reveal_key<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, scope, "api-keys", api_keys.reveal(&id)) }).await
}

async fn retire_client<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(
        async move { operations!(state, scope, "oauth-clients", oauth_clients.retire(&id)) },
    )
    .await
}

async fn list_members<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
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
        operations!(state, scope, "members", members.list(query))
    })
    .await
}

async fn get_member<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path((id, user_id)): Path<(String, String)>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, scope, "members", members.get(&id, &user_id)) })
        .await
}

async fn add_member<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
    Json(write): Json<MemberWrite>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, scope, "members", members.add(&id, write)) }).await
}

async fn set_member_role<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path((id, user_id)): Path<(String, String)>,
    Json(patch): Json<gproxy_app::dto::MemberPatch>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        operations!(
            state,
            scope,
            "members",
            members.set_role(&id, &user_id, patch)
        )
    })
    .await
}

async fn remove_member<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path((id, user_id)): Path<(String, String)>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        operations!(@empty state, scope, "members", members.remove(&id, &user_id))
    })
    .await
}

async fn list_team_members<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
    Query(mut query): Query<ListQuery>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        query.team_id = Some(id);
        operations!(state, scope, "team-members", team_members.list(query))
    })
    .await
}

async fn get_team_member<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path((id, user_id)): Path<(String, String)>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        operations!(
            state,
            scope,
            "team-members",
            team_members.get(&id, &user_id)
        )
    })
    .await
}

async fn add_team_member<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
    Json(write): Json<MemberWrite>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(
        async move { operations!(state, scope, "team-members", team_members.add(&id, write)) },
    )
    .await
}

async fn set_team_member_role<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path((id, user_id)): Path<(String, String)>,
    Json(patch): Json<gproxy_app::dto::MemberPatch>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        operations!(
            state,
            scope,
            "team-members",
            team_members.set_role(&id, &user_id, patch)
        )
    })
    .await
}

async fn remove_team_member<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path((id, user_id)): Path<(String, String)>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        operations!(@empty state, scope, "team-members", team_members.remove(&id, &user_id))
    })
    .await
}

async fn list_sessions<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Query(query): Query<ListQuery>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, scope, "sessions", sessions.page(query)) }).await
}

async fn revoke_session<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(@empty state, scope, "sessions", sessions.revoke(&id)) })
        .await
}

async fn user_sessions<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, scope, "sessions", sessions.list(&id)) }).await
}

async fn revoke_user_sessions<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        gate!("sessions", scope);
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
    Extension(scope): Extension<AdminScope>,
    Query(query): Query<AuditQuery>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move { operations!(state, scope, "audit", audit.query(query)) }).await
}
