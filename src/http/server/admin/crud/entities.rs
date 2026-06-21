//! Macro-generated CRUD handlers for the standard global entities.
//!
//! [`crud_entity!`] emits a submodule with `list` / `get` / `upsert` / `delete`
//! axum handlers for one entity. Two GET-by-id strategies are supported:
//! `get = $m` (a backend `get_X(id)` method exists) and `find` (no by-id
//! getter — scan `list_X()` and match on `id`, used for aliases).

use super::{internal, upsert_err};
use crate::admin::invalidate;
use crate::api::error::ApiError;
use crate::app::AppState;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

/// Generate the four CRUD handlers for one entity in a named submodule.
macro_rules! crud_entity {
    (
        mod $modname:ident,
        record = $record:ty,
        input = $input:ty,
        list = $list:ident,
        get = $getter:tt,
        upsert = $upsert:ident,
        delete = $delete:ident $(,)?
    ) => {
        pub mod $modname {
            #[allow(unused_imports)]
            use super::*;

            /// `GET /…` — list all records.
            pub async fn list(
                State(state): State<AppState>,
            ) -> Result<Json<Vec<$record>>, ApiError> {
                Ok(Json(state.persistence.$list().await.map_err(internal)?))
            }

            crud_entity!(@get $record, $list, $getter);

            /// `POST /…` — insert (`id=None`) or update (`Some(id)`), then invalidate.
            pub async fn upsert(
                State(state): State<AppState>,
                Json(input): Json<$input>,
            ) -> Result<Json<$record>, ApiError> {
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

    // GET-by-id with no backend getter: scan the list and match on `id`.
    // (Literal `find` arm must precede the generic `$getter:ident` arm.)
    (@get $record:ty, $list:ident, find) => {
        /// `GET /…/{id}` — no by-id getter, so scan the list.
        pub async fn get(
            State(state): State<AppState>,
            Path(id): Path<i64>,
        ) -> Result<Json<$record>, ApiError> {
            let all = state.persistence.$list().await.map_err(internal)?;
            match all.into_iter().find(|r| r.id == id) {
                Some(rec) => Ok(Json(rec)),
                None => Err(ApiError::NotFound("not found".into())),
            }
        }
    };

    // GET-by-id via a backend `get_X(id)` method.
    (@get $record:ty, $list:ident, $getter:ident) => {
        /// `GET /…/{id}` — via the backend by-id getter.
        pub async fn get(
            State(state): State<AppState>,
            Path(id): Path<i64>,
        ) -> Result<Json<$record>, ApiError> {
            match state.persistence.$getter(id).await.map_err(internal)? {
                Some(rec) => Ok(Json(rec)),
                None => Err(ApiError::NotFound("not found".into())),
            }
        }
    };
}

use crate::store::persistence::records::{
    Alias, AliasInput, InstanceSettings, InstanceSettingsInput, Org, OrgInput, Provider,
    ProviderInput, Route, RouteInput, RuleSet, RuleSetInput,
};

crud_entity!(
    mod orgs,
    record = Org,
    input = OrgInput,
    list = list_orgs,
    get = get_org,
    upsert = upsert_org,
    delete = delete_org,
);

// providers is hand-rolled (not crud_entity!): creation seeds the channel's
// default routing rules (so GET /routing-rules is a plain load), and the channel
// is immutable after creation.
pub mod providers {
    use super::*;

    pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<Provider>>, ApiError> {
        Ok(Json(
            state.persistence.list_providers().await.map_err(internal)?,
        ))
    }

    pub async fn get(
        State(state): State<AppState>,
        Path(id): Path<i64>,
    ) -> Result<Json<Provider>, ApiError> {
        match state.persistence.get_provider(id).await.map_err(internal)? {
            Some(rec) => Ok(Json(rec)),
            None => Err(ApiError::NotFound("not found".into())),
        }
    }

    pub async fn upsert(
        State(state): State<AppState>,
        Json(input): Json<ProviderInput>,
    ) -> Result<Json<Provider>, ApiError> {
        let is_create = input.id.is_none();
        // Channel is immutable after creation (it determines the default routing).
        if let Some(id) = input.id {
            let existing = state
                .persistence
                .get_provider(id)
                .await
                .map_err(internal)?
                .ok_or_else(|| ApiError::NotFound("provider not found".into()))?;
            if existing.channel != input.channel {
                return Err(ApiError::BadRequest(
                    "channel cannot be changed after creation".into(),
                ));
            }
        }
        let rec = state
            .persistence
            .upsert_provider(input)
            .await
            .map_err(upsert_err)?;
        if is_create {
            crate::api::routing::seed_default_routing(
                state.persistence.as_ref(),
                state.channels.as_ref(),
                rec.id,
                false,
            )
            .await?;
        }
        invalidate(&state).await;
        Ok(Json(rec))
    }

    pub async fn delete(
        State(state): State<AppState>,
        Path(id): Path<i64>,
    ) -> Result<axum::response::Response, ApiError> {
        if state
            .persistence
            .delete_provider(id)
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

crud_entity!(
    mod routes,
    record = Route,
    input = RouteInput,
    list = list_routes,
    get = get_route,
    upsert = upsert_route,
    delete = delete_route,
);

crud_entity!(
    mod aliases,
    record = Alias,
    input = AliasInput,
    list = list_aliases,
    get = find,
    upsert = upsert_alias,
    delete = delete_alias,
);

crud_entity!(
    mod rule_sets,
    record = RuleSet,
    input = RuleSetInput,
    list = list_rule_sets,
    get = get_rule_set,
    upsert = upsert_rule_set,
    delete = delete_rule_set,
);

/// Instance settings have no per-id routes: `GET` returns the list, `POST`
/// upserts. The macro's `list` / `upsert` are reused; `get` / `delete` are not
/// mounted, so a `find` strategy is supplied but unused.
pub mod instance_settings {
    use super::*;

    /// `GET /admin/instance-settings` — list all instance settings.
    pub async fn list(
        State(state): State<AppState>,
    ) -> Result<Json<Vec<InstanceSettings>>, ApiError> {
        Ok(Json(
            state
                .persistence
                .list_instance_settings()
                .await
                .map_err(internal)?,
        ))
    }

    /// `POST /admin/instance-settings` — upsert, then invalidate.
    pub async fn upsert(
        State(state): State<AppState>,
        Json(input): Json<InstanceSettingsInput>,
    ) -> Result<Json<InstanceSettings>, ApiError> {
        let rec = state
            .persistence
            .upsert_instance_settings(input)
            .await
            .map_err(upsert_err)?;
        invalidate(&state).await;
        Ok(Json(rec))
    }
}
