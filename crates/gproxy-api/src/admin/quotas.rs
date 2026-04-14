use crate::auth::authorize_admin;
use crate::error::{AckResponse, HttpError};
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use gproxy_server::AppState;
use gproxy_storage::Scope;
use gproxy_storage::repository::QuotaRepository;
use std::sync::Arc;

#[derive(serde::Serialize, Debug, Clone, PartialEq)]
pub struct MemoryUserQuotaRow {
    pub user_id: i64,
    pub quota: f64,
    pub cost_used: f64,
    pub remaining: f64,
}

#[derive(serde::Deserialize, Default)]
pub struct UserQuotaQueryParams {
    #[serde(default)]
    pub user_id: Scope<i64>,
    pub limit: Option<usize>,
}

pub async fn query_user_quotas(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<UserQuotaQueryParams>,
) -> Result<Json<Vec<MemoryUserQuotaRow>>, HttpError> {
    authorize_admin(&headers, &state)?;
    let mut rows: Vec<MemoryUserQuotaRow> = state
        .storage()
        .list_user_quotas()
        .await?
        .into_iter()
        .filter(|row| match &query.user_id {
            Scope::Eq(user_id) => row.user_id == *user_id,
            _ => true,
        })
        .map(|row| MemoryUserQuotaRow {
            user_id: row.user_id,
            quota: row.quota,
            cost_used: row.cost_used,
            remaining: (row.quota - row.cost_used).max(0.0),
        })
        .collect();
    rows.sort_by_key(|row| row.user_id);
    if let Some(limit) = query.limit {
        rows.truncate(limit);
    }
    Ok(Json(rows))
}

pub async fn upsert_user_quota(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<gproxy_storage::UserQuotaWrite>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    state.storage().upsert_user_quota(payload.clone()).await?;
    state.upsert_user_quota_in_memory(payload.user_id, payload.quota, payload.cost_used);
    Ok(Json(AckResponse { ok: true, id: None }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{Json, extract::State};
    use http::HeaderMap;

    use super::{UserQuotaQueryParams, query_user_quotas, upsert_user_quota};
    use gproxy_server::{AppState, AppStateBuilder, GlobalConfig};
    use gproxy_storage::{
        Scope, SeaOrmStorage, UserQuotaWrite,
        repository::{QuotaRepository, UserRepository},
    };

    async fn build_test_state() -> Arc<AppState> {
        let storage = Arc::new(
            SeaOrmStorage::connect("sqlite::memory:", None)
                .await
                .expect("in-memory sqlite storage"),
        );
        storage.sync().await.expect("sync schema");
        storage
            .upsert_user(gproxy_storage::UserWrite {
                id: 1,
                name: "admin".to_string(),
                password: crate::login::hash_password("admin-password"),
                enabled: true,
                is_admin: true,
            })
            .await
            .expect("seed admin");
        storage
            .upsert_user_key(gproxy_storage::UserKeyWrite {
                id: 10,
                user_id: 1,
                api_key: "sk-admin".to_string(),
                label: Some("admin".to_string()),
                enabled: true,
            })
            .await
            .expect("seed admin key");
        storage
            .upsert_user(gproxy_storage::UserWrite {
                id: 2,
                name: "alice".to_string(),
                password: crate::login::hash_password("user-password"),
                enabled: true,
                is_admin: false,
            })
            .await
            .expect("seed user");
        storage
            .upsert_user_quota(UserQuotaWrite {
                user_id: 2,
                quota: 25.0,
                cost_used: 4.5,
            })
            .await
            .expect("seed user quota");

        let state = Arc::new(
            AppStateBuilder::new()
                .engine(gproxy_sdk::engine::engine::GproxyEngine::builder().build())
                .storage(storage)
                .config(GlobalConfig {
                    dsn: "sqlite::memory:".to_string(),
                    ..GlobalConfig::default()
                })
                .build(),
        );
        crate::bootstrap::reload_from_db(&state)
            .await
            .expect("reload state");
        state
    }

    fn admin_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer sk-admin".parse().expect("header"));
        headers
    }

    #[tokio::test]
    async fn query_user_quotas_filters_and_computes_remaining() {
        let state = build_test_state().await;

        let Json(rows) = query_user_quotas(
            State(state),
            admin_headers(),
            Json(UserQuotaQueryParams {
                user_id: Scope::Eq(2),
                limit: None,
            }),
        )
        .await
        .expect("query quotas");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].user_id, 2);
        assert_eq!(rows[0].quota, 25.0);
        assert_eq!(rows[0].cost_used, 4.5);
        assert_eq!(rows[0].remaining, 20.5);
    }

    #[tokio::test]
    async fn upsert_user_quota_persists_and_updates_memory() {
        let state = build_test_state().await;

        let Json(_ack) = upsert_user_quota(
            State(state.clone()),
            admin_headers(),
            Json(UserQuotaWrite {
                user_id: 2,
                quota: 30.0,
                cost_used: 12.25,
            }),
        )
        .await
        .expect("upsert quota");

        assert_eq!(state.get_user_quota(2), (30.0, 12.25));

        let stored = state
            .storage()
            .list_user_quotas()
            .await
            .expect("list user quotas");
        let alice = stored
            .into_iter()
            .find(|row| row.user_id == 2)
            .expect("alice quota row");
        assert_eq!(alice.quota, 30.0);
        assert_eq!(alice.cost_used, 12.25);
    }
}
