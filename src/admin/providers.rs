use axum::Router;
use axum::extract::{Extension, Json, Path};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, put};
use serde_json::Value;

use crate::context::AppContext;
use crate::providers::admin::ProviderAdmin;
#[cfg(feature = "provider-aistudio")]
use crate::providers::aistudio;
#[cfg(feature = "provider-antigravity")]
use crate::providers::antigravity;
#[cfg(feature = "provider-claude")]
use crate::providers::claude;
#[cfg(feature = "provider-claudecode")]
use crate::providers::claudecode;
#[cfg(feature = "provider-codex")]
use crate::providers::codex;
#[cfg(feature = "provider-deepseek")]
use crate::providers::deepseek;
#[cfg(feature = "provider-geminicli")]
use crate::providers::geminicli;
#[cfg(feature = "provider-nvidia")]
use crate::providers::nvidia;
#[cfg(feature = "provider-openai")]
use crate::providers::openai;
#[cfg(feature = "provider-vertex")]
use crate::providers::vertex;
#[cfg(feature = "provider-vertexexpress")]
use crate::providers::vertexexpress;

pub fn router() -> Router {
    Router::new()
        .route(
            "/{name}/config",
            get(get_provider_config).put(put_provider_config),
        )
        .route(
            "/{name}/credentials",
            get(list_provider_credentials).post(add_provider_credential),
        )
        .route(
            "/{name}/credentials/{index}",
            put(update_provider_credential).delete(delete_provider_credential),
        )
}

fn provider_admin(name: &str) -> Option<&'static dyn ProviderAdmin> {
    match name {
        #[cfg(feature = "provider-openai")]
        "openai" => Some(&openai::admin::OpenAIAdmin),
        #[cfg(feature = "provider-codex")]
        "codex" => Some(&codex::admin::CodexAdmin),
        #[cfg(feature = "provider-claude")]
        "claude" => Some(&claude::admin::ClaudeAdmin),
        #[cfg(feature = "provider-claudecode")]
        "claudecode" => Some(&claudecode::admin::ClaudeCodeAdmin),
        #[cfg(feature = "provider-aistudio")]
        "aistudio" => Some(&aistudio::admin::AIStudioAdmin),
        #[cfg(feature = "provider-vertex")]
        "vertex" => Some(&vertex::admin::VertexAdmin),
        #[cfg(feature = "provider-vertexexpress")]
        "vertexexpress" => Some(&vertexexpress::admin::VertexExpressAdmin),
        #[cfg(feature = "provider-geminicli")]
        "geminicli" => Some(&geminicli::admin::GeminiCliAdmin),
        #[cfg(feature = "provider-antigravity")]
        "antigravity" => Some(&antigravity::admin::AntigravityAdmin),
        #[cfg(feature = "provider-nvidia")]
        "nvidia" => Some(&nvidia::admin::NvidiaAdmin),
        #[cfg(feature = "provider-deepseek")]
        "deepseek" => Some(&deepseek::admin::DeepSeekAdmin),
        _ => None,
    }
}

async fn get_provider_config(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    super::ensure_admin(&headers, &ctx)?;
    let admin = provider_admin(&name).ok_or(StatusCode::NOT_FOUND)?;
    let config = admin.get_config(&ctx).await?;
    Ok(Json(config))
}

async fn put_provider_config(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(config): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    super::ensure_admin(&headers, &ctx)?;
    let admin = provider_admin(&name).ok_or(StatusCode::NOT_FOUND)?;
    let config = admin.put_config(&ctx, config).await?;
    Ok(Json(config))
}

async fn list_provider_credentials(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    super::ensure_admin(&headers, &ctx)?;
    let admin = provider_admin(&name).ok_or(StatusCode::NOT_FOUND)?;
    let credentials = admin.list_credentials(&ctx).await?;
    Ok(Json(credentials))
}

async fn add_provider_credential(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(credential): Json<Value>,
) -> Result<StatusCode, StatusCode> {
    super::ensure_admin(&headers, &ctx)?;
    let admin = provider_admin(&name).ok_or(StatusCode::NOT_FOUND)?;
    admin.add_credential(&ctx, credential).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_provider_credential(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Path((name, index)): Path<(String, usize)>,
    Json(credential): Json<Value>,
) -> Result<StatusCode, StatusCode> {
    super::ensure_admin(&headers, &ctx)?;
    let admin = provider_admin(&name).ok_or(StatusCode::NOT_FOUND)?;
    admin.update_credential(&ctx, index, credential).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_provider_credential(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Path((name, index)): Path<(String, usize)>,
) -> Result<StatusCode, StatusCode> {
    super::ensure_admin(&headers, &ctx)?;
    let admin = provider_admin(&name).ok_or(StatusCode::NOT_FOUND)?;
    admin.delete_credential(&ctx, index).await?;
    Ok(StatusCode::NO_CONTENT)
}
use std::sync::Arc;
