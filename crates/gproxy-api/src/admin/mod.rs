use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post};

use gproxy_server::AppState;

pub mod config_toml;
pub mod credentials;
pub mod file_permissions;
pub mod health;
pub mod models;
pub mod permissions;
pub mod providers;
pub mod quotas;
pub mod rate_limits;
pub mod reload;
pub mod requests;
pub mod settings;
pub mod update;
pub mod usages;
pub mod users;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        // Health
        .route("/health", get(health::health))
        // Reload all caches from database
        .route("/reload", post(reload::reload))
        // Global settings
        .route("/global-settings", get(settings::get_global_settings))
        .route(
            "/global-settings/upsert",
            post(settings::upsert_global_settings),
        )
        // Providers
        .route("/providers/query", post(providers::query_providers))
        .route(
            "/providers/default-dispatch",
            post(providers::default_provider_dispatch),
        )
        .route("/providers/upsert", post(providers::upsert_provider))
        .route("/providers/delete", post(providers::delete_provider))
        .route(
            "/providers/batch-upsert",
            post(providers::batch_upsert_providers),
        )
        .route(
            "/providers/batch-delete",
            post(providers::batch_delete_providers),
        )
        // Credentials
        .route("/credentials/query", post(credentials::query_credentials))
        .route("/credentials/upsert", post(credentials::upsert_credential))
        .route("/credentials/delete", post(credentials::delete_credential))
        .route(
            "/credentials/batch-upsert",
            post(credentials::batch_upsert_credentials),
        )
        .route(
            "/credentials/batch-delete",
            post(credentials::batch_delete_credentials),
        )
        .route(
            "/credential-statuses/query",
            post(credentials::query_credential_statuses),
        )
        .route(
            "/credential-statuses/update",
            post(credentials::update_credential_status),
        )
        // Models
        .route("/models/query", post(models::query_models))
        .route("/models/upsert", post(models::upsert_model))
        .route("/models/delete", post(models::delete_model))
        .route("/models/batch-upsert", post(models::batch_upsert_models))
        .route("/models/batch-delete", post(models::batch_delete_models))
        .route("/model-aliases/query", post(models::query_model_aliases))
        .route("/model-aliases/upsert", post(models::upsert_model_alias))
        .route("/model-aliases/delete", post(models::delete_model_alias))
        .route(
            "/model-aliases/batch-upsert",
            post(models::batch_upsert_model_aliases),
        )
        .route(
            "/model-aliases/batch-delete",
            post(models::batch_delete_model_aliases),
        )
        // Users
        .route("/users/query", post(users::query_users))
        .route("/users/upsert", post(users::upsert_user))
        .route("/users/delete", post(users::delete_user))
        .route("/users/batch-upsert", post(users::batch_upsert_users))
        .route("/users/batch-delete", post(users::batch_delete_users))
        .route("/user-keys/query", post(users::query_user_keys))
        .route("/user-keys/generate", post(users::generate_user_key))
        .route(
            "/user-keys/update-enabled",
            post(users::update_user_key_enabled),
        )
        .route("/user-keys/delete", post(users::delete_user_key))
        .route("/user-quotas/query", post(quotas::query_user_quotas))
        .route("/user-quotas/upsert", post(quotas::upsert_user_quota))
        .route(
            "/user-keys/batch-upsert",
            post(users::batch_upsert_user_keys),
        )
        .route(
            "/user-keys/batch-delete",
            post(users::batch_delete_user_keys),
        )
        // Permissions
        .route(
            "/user-permissions/query",
            post(permissions::query_permissions),
        )
        .route(
            "/user-permissions/upsert",
            post(permissions::upsert_permission),
        )
        .route(
            "/user-permissions/delete",
            post(permissions::delete_permission),
        )
        .route(
            "/user-permissions/batch-upsert",
            post(permissions::batch_upsert_permissions),
        )
        .route(
            "/user-permissions/batch-delete",
            post(permissions::batch_delete_permissions),
        )
        // File permissions
        .route(
            "/user-file-permissions/query",
            post(file_permissions::query_file_permissions),
        )
        .route(
            "/user-file-permissions/upsert",
            post(file_permissions::upsert_file_permission),
        )
        .route(
            "/user-file-permissions/delete",
            post(file_permissions::delete_file_permission),
        )
        .route(
            "/user-file-permissions/batch-upsert",
            post(file_permissions::batch_upsert_file_permissions),
        )
        .route(
            "/user-file-permissions/batch-delete",
            post(file_permissions::batch_delete_file_permissions),
        )
        // Rate limits
        .route(
            "/user-rate-limits/query",
            post(rate_limits::query_rate_limits),
        )
        .route(
            "/user-rate-limits/upsert",
            post(rate_limits::upsert_rate_limit),
        )
        .route(
            "/user-rate-limits/delete",
            post(rate_limits::delete_rate_limit),
        )
        .route(
            "/user-rate-limits/batch-upsert",
            post(rate_limits::batch_upsert_rate_limits),
        )
        .route(
            "/user-rate-limits/batch-delete",
            post(rate_limits::batch_delete_rate_limits),
        )
        // Requests
        .route(
            "/requests/upstream/query",
            post(requests::query_upstream_requests),
        )
        .route(
            "/requests/upstream/count",
            post(requests::count_upstream_requests),
        )
        .route(
            "/requests/upstream/delete",
            post(requests::delete_upstream_requests),
        )
        .route(
            "/requests/upstream/batch-delete",
            post(requests::batch_delete_upstream_requests),
        )
        .route(
            "/requests/downstream/query",
            post(requests::query_downstream_requests),
        )
        .route(
            "/requests/downstream/count",
            post(requests::count_downstream_requests),
        )
        .route(
            "/requests/downstream/delete",
            post(requests::delete_downstream_requests),
        )
        .route(
            "/requests/downstream/batch-delete",
            post(requests::batch_delete_downstream_requests),
        )
        // Usages
        .route("/usages/query", post(usages::query_usages))
        .route("/usages/count", post(usages::count_usages))
        .route("/usages/batch-delete", post(usages::batch_delete_usages))
        // Config export
        .route("/config/export-toml", post(config_toml::export_toml))
        // Self-update
        .route("/update/check", post(update::check_update))
        .route("/update", post(update::perform_update))
}
