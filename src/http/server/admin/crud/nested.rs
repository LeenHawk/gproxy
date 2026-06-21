//! Macro-generated CRUD handlers for the parent-nested config entities
//! (no secrets): `list_X(parent_id)` / `upsert_X(Input)` / `delete_X(id)`.
//!
//! [`crud_nested!`] emits a submodule with `list` (parent path param →
//! `list_X(parent)`), `upsert` (takes the `*Input` directly), and `delete` (by
//! id). Both `upsert` and `delete` invalidate the snapshot. These records carry
//! no secrets, so the records are serialized directly (no redacting view).

use super::{internal, upsert_err};
use crate::admin::invalidate;
use crate::api::error::ApiError;
use crate::app::AppState;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

/// Generate the three nested CRUD handlers for one entity in a named submodule.
/// `parent` names the input's parent-fk field; `upsert` rejects a body whose
/// fk doesn't match the URL parent id (the URL is authoritative — §8 admin
/// semantics and audit trails key off the path).
macro_rules! crud_nested {
    (
        mod $modname:ident,
        record = $record:ty,
        input = $input:ty,
        parent = $parent:ident,
        list = $list:ident,
        upsert = $upsert:ident,
        delete = $delete:ident $(,)?
    ) => {
        pub mod $modname {
            #[allow(unused_imports)]
            use super::*;

            /// `GET /…/{parent}/…` — list the records under one parent.
            pub async fn list(
                State(state): State<AppState>,
                Path(parent_id): Path<i64>,
            ) -> Result<Json<Vec<$record>>, ApiError> {
                Ok(Json(
                    state.persistence.$list(parent_id).await.map_err(internal)?,
                ))
            }

            /// `POST /…/{parent}/…` — upsert the `*Input` (its parent fk must
            /// match the URL parent id), then invalidate.
            pub async fn upsert(
                State(state): State<AppState>,
                Path(parent_id): Path<i64>,
                Json(input): Json<$input>,
            ) -> Result<Json<$record>, ApiError> {
                if input.$parent != parent_id {
                    return Err(ApiError::BadRequest(format!(
                        "body {} {} does not match URL parent {}",
                        stringify!($parent),
                        input.$parent,
                        parent_id
                    )));
                }
                let rec = state.persistence.$upsert(input).await.map_err(upsert_err)?;
                invalidate(&state).await;
                Ok(Json(rec))
            }

            /// `DELETE /…/{id}` — 204 on removal, 404 otherwise; invalidate on hit.
            pub async fn delete(
                State(state): State<AppState>,
                Path(id): Path<i64>,
            ) -> Result<axum::response::Response, ApiError> {
                if state.persistence.$delete(id).await.map_err(internal)? {
                    invalidate(&state).await;
                    Ok(StatusCode::NO_CONTENT.into_response())
                } else {
                    Err(ApiError::NotFound("not found".into()))
                }
            }
        }
    };
}

use crate::store::persistence::records::{
    ProviderModel, ProviderModelInput, ProviderRuleSet, ProviderRuleSetInput, RouteMember,
    RouteMemberInput, RoutingRule, RoutingRuleInput, Rule, RuleInput, Team, TeamInput,
};

crud_nested!(
    mod teams,
    record = Team,
    input = TeamInput,
    parent = org_id,
    list = list_teams,
    upsert = upsert_team,
    delete = delete_team,
);

crud_nested!(
    mod provider_models,
    record = ProviderModel,
    input = ProviderModelInput,
    parent = provider_id,
    list = list_provider_models,
    upsert = upsert_provider_model,
    delete = delete_provider_model,
);

crud_nested!(
    mod route_members,
    record = RouteMember,
    input = RouteMemberInput,
    parent = route_id,
    list = list_route_members,
    upsert = upsert_route_member,
    delete = delete_route_member,
);

crud_nested!(
    mod rules,
    record = Rule,
    input = RuleInput,
    parent = rule_set_id,
    list = list_rules,
    upsert = upsert_rule,
    delete = delete_rule,
);

// routing-rules: /admin/providers/{provider_id}/routing-rules
//                DELETE /admin/routing-rules/{id}
//                POST   /admin/providers/{provider_id}/routing-rules/reset
//
// Hand-rolled (not crud_nested!): GET is a plain load (the default rules are
// materialized at provider creation); `reset` re-seeds the channel defaults.
pub mod routing_rules {
    use super::*;

    /// `GET /admin/providers/{provider_id}/routing-rules` — load the stored rules.
    pub async fn list(
        State(state): State<AppState>,
        Path(provider_id): Path<i64>,
    ) -> Result<Json<Vec<RoutingRule>>, ApiError> {
        Ok(Json(
            state
                .persistence
                .list_routing_rules(provider_id)
                .await
                .map_err(internal)?,
        ))
    }

    /// `POST /admin/providers/{provider_id}/routing-rules` — upsert a rule.
    pub async fn upsert(
        State(state): State<AppState>,
        Path(parent_id): Path<i64>,
        Json(input): Json<RoutingRuleInput>,
    ) -> Result<Json<RoutingRule>, ApiError> {
        if input.provider_id != parent_id {
            return Err(ApiError::BadRequest(format!(
                "body provider_id {} does not match URL parent {parent_id}",
                input.provider_id
            )));
        }
        let rec = state
            .persistence
            .upsert_routing_rule(input)
            .await
            .map_err(upsert_err)?;
        invalidate(&state).await;
        Ok(Json(rec))
    }

    /// `POST /admin/providers/{provider_id}/routing-rules/reset` — recompute the
    /// channel's default rules (create-or-overwrite the 3×4 cells), returning the
    /// resulting rule set.
    pub async fn reset(
        State(state): State<AppState>,
        Path(provider_id): Path<i64>,
    ) -> Result<Json<Vec<RoutingRule>>, ApiError> {
        crate::api::routing::seed_default_routing(
            state.persistence.as_ref(),
            state.channels.as_ref(),
            provider_id,
            true,
        )
        .await?;
        invalidate(&state).await;
        Ok(Json(
            state
                .persistence
                .list_routing_rules(provider_id)
                .await
                .map_err(internal)?,
        ))
    }

    /// `DELETE /admin/routing-rules/{id}` — 204 on removal, 404 otherwise.
    pub async fn delete(
        State(state): State<AppState>,
        Path(id): Path<i64>,
    ) -> Result<axum::response::Response, ApiError> {
        if state
            .persistence
            .delete_routing_rule(id)
            .await
            .map_err(internal)?
        {
            invalidate(&state).await;
            Ok(StatusCode::NO_CONTENT.into_response())
        } else {
            Err(ApiError::NotFound("not found".into()))
        }
    }
}

crud_nested!(
    mod provider_rule_sets,
    record = ProviderRuleSet,
    input = ProviderRuleSetInput,
    parent = provider_id,
    list = list_provider_rule_sets,
    upsert = upsert_provider_rule_set,
    delete = delete_provider_rule_set,
);
