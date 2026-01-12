use std::sync::Arc;

use axum::Router;
use axum::extract::{Extension, Path, Query};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::get;
use serde::Deserialize;

use crate::context::AppContext;
use crate::providers::credential_status::ProviderKind;
use crate::providers::usage::UsageViewRecord;

pub fn router() -> Router {
    Router::new()
        .route("/", get(get_usage_by_api_key))
        .route("/provider/{name}", get(get_usage_by_provider_credential))
}

#[derive(Deserialize)]
struct ApiKeyQuery {
    api_key: String,
}

#[derive(Deserialize)]
struct CredentialQuery {
    credential_id: String,
}

async fn get_usage_by_api_key(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<ApiKeyQuery>,
) -> Result<axum::Json<Vec<UsageViewRecord>>, StatusCode> {
    super::ensure_admin(&headers, &ctx)?;
    let store = ctx.usage_store();
    let records = store
        .query_by_api_key(query.api_key.as_str())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(axum::Json(records))
}

async fn get_usage_by_provider_credential(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Query(query): Query<CredentialQuery>,
) -> Result<axum::Json<Vec<UsageViewRecord>>, StatusCode> {
    super::ensure_admin(&headers, &ctx)?;
    let provider = provider_kind(&name).ok_or(StatusCode::NOT_FOUND)?;
    let store = ctx.usage_store();
    let records = store
        .query_by_provider_credential(provider, query.credential_id.as_str())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(axum::Json(records))
}

fn provider_kind(name: &str) -> Option<ProviderKind> {
    match name {
        "openai" => Some(ProviderKind::OpenAI),
        "claude" => Some(ProviderKind::Claude),
        "aistudio" => Some(ProviderKind::AIStudio),
        "deepseek" => Some(ProviderKind::DeepSeek),
        "nvidia" => Some(ProviderKind::Nvidia),
        "vertexexpress" => Some(ProviderKind::VertexExpress),
        "claudecode" => Some(ProviderKind::ClaudeCode),
        "codex" => Some(ProviderKind::Codex),
        "vertex" => Some(ProviderKind::Vertex),
        "geminicli" => Some(ProviderKind::GeminiCli),
        "antigravity" => Some(ProviderKind::Antigravity),
        _ => None,
    }
}
