use crate::auth::authorize_admin;
use crate::error::HttpError;
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use gproxy_server::AppState;
use gproxy_storage::*;
use std::sync::Arc;

pub async fn overview(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<DashboardQuery>,
) -> Result<Json<DashboardOverview>, HttpError> {
    authorize_admin(&headers, &state)?;
    let result = state.storage().query_dashboard_overview(&query).await?;
    Ok(Json(result))
}

pub async fn top_providers(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<DashboardQuery>,
) -> Result<Json<DashboardTopProviders>, HttpError> {
    authorize_admin(&headers, &state)?;
    let result = state
        .storage()
        .query_dashboard_top_providers(&query)
        .await?;
    Ok(Json(result))
}

pub async fn top_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<DashboardQuery>,
) -> Result<Json<DashboardTopModels>, HttpError> {
    authorize_admin(&headers, &state)?;
    let result = state.storage().query_dashboard_top_models(&query).await?;
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{Json, extract::State, http::HeaderMap};
    use gproxy_server::{AppState, AppStateBuilder, GlobalConfig};
    use gproxy_storage::{
        SeaOrmStorage,
        entities::{downstream_requests, providers, upstream_requests, usages},
        repository::UserRepository,
    };
    use sea_orm::ActiveValue::{NotSet, Set};
    use sea_orm::EntityTrait;
    use serde_json::json;
    use time::{Duration, OffsetDateTime};

    use super::{DashboardQuery, overview, top_providers};

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

        let at = OffsetDateTime::UNIX_EPOCH + Duration::hours(24);
        let provider_id = providers::Entity::insert(providers::ActiveModel {
            id: NotSet,
            name: Set("demo".to_string()),
            channel: Set("openai".to_string()),
            label: Set(None),
            settings_json: Set(json!({})),
            routing_json: Set(json!({})),
            created_at: Set(at),
            updated_at: Set(at),
        })
        .exec(storage.connection())
        .await
        .expect("insert provider")
        .last_insert_id;

        downstream_requests::Entity::insert(downstream_requests::ActiveModel {
            trace_id: NotSet,
            at: Set(at),
            internal: Set(false),
            user_id: Set(None),
            user_key_id: Set(None),
            request_method: Set("POST".to_string()),
            request_headers_json: Set(json!({})),
            request_path: Set("/v1/chat/completions".to_string()),
            request_query: Set(None),
            request_body: Set(None),
            response_status: Set(Some(200)),
            response_headers_json: Set(json!({})),
            response_body: Set(None),
            created_at: Set(at),
        })
        .exec(storage.connection())
        .await
        .expect("insert downstream request");

        usages::Entity::insert(usages::ActiveModel {
            trace_id: NotSet,
            downstream_trace_id: Set(None),
            at: Set(at),
            provider_id: Set(Some(provider_id)),
            credential_id: Set(None),
            user_id: Set(None),
            user_key_id: Set(None),
            operation: Set("chat".to_string()),
            protocol: Set("openai".to_string()),
            model: Set(Some("gpt-4o".to_string())),
            input_tokens: Set(Some(120)),
            output_tokens: Set(Some(40)),
            cache_read_input_tokens: Set(None),
            cache_creation_input_tokens: Set(None),
            cache_creation_input_tokens_5min: Set(None),
            cache_creation_input_tokens_1h: Set(None),
            cost: Set(2.5),
            created_at: Set(at),
        })
        .exec(storage.connection())
        .await
        .expect("insert usage");

        upstream_requests::Entity::insert(upstream_requests::ActiveModel {
            trace_id: NotSet,
            downstream_trace_id: Set(None),
            at: Set(at),
            internal: Set(false),
            provider_id: Set(Some(provider_id)),
            credential_id: Set(None),
            request_method: Set("POST".to_string()),
            request_headers_json: Set(json!({})),
            request_url: Set(Some("https://example.test".to_string())),
            request_body: Set(None),
            response_status: Set(Some(200)),
            response_headers_json: Set(json!({})),
            response_body: Set(None),
            initial_latency_ms: Set(Some(100)),
            total_latency_ms: Set(Some(300)),
            created_at: Set(at),
        })
        .exec(storage.connection())
        .await
        .expect("insert upstream request");

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
    async fn overview_returns_kpis_and_series_for_admin() {
        let state = build_test_state().await;
        let at = (OffsetDateTime::UNIX_EPOCH + Duration::hours(24)).unix_timestamp() * 1000;

        let Json(result) = overview(
            State(state),
            admin_headers(),
            Json(DashboardQuery {
                from_unix_ms: at,
                to_unix_ms: at + 60 * 60 * 1000,
                bucket_seconds: 3600,
            }),
        )
        .await
        .expect("overview");

        assert_eq!(result.kpi.total_requests, 1);
        assert_eq!(result.kpi.total_cost, 2.5);
        assert_eq!(result.kpi.avg_latency_ms, Some(300.0));
        assert_eq!(result.traffic.len(), 1);
        assert_eq!(result.status_codes.len(), 1);
    }

    #[tokio::test]
    async fn top_providers_returns_channel_breakdown() {
        let state = build_test_state().await;
        let at = (OffsetDateTime::UNIX_EPOCH + Duration::hours(24)).unix_timestamp() * 1000;

        let Json(result) = top_providers(
            State(state),
            admin_headers(),
            Json(DashboardQuery {
                from_unix_ms: at,
                to_unix_ms: at + 60 * 60 * 1000,
                bucket_seconds: 3600,
            }),
        )
        .await
        .expect("top providers");

        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].channel.as_deref(), Some("openai"));
        assert_eq!(result.rows[0].request_count, 1);
    }
}
