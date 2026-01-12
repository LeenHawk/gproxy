use axum::http::StatusCode;
use url::Url;

use crate::context::AppContext;
use crate::providers::endpoints::GeminiVersion;
use crate::providers::vertex::VertexProvider;

mod claude;
mod gemini;
mod openai;

pub(super) async fn get_settings_and_credentials(
    ctx: &AppContext,
) -> Result<VertexProvider, StatusCode> {
    let settings = ctx
        .vertex()
        .get_config()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let credentials = ctx
        .vertex()
        .get_credentials()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(VertexProvider {
        setting: settings,
        credentials,
        ..Default::default()
    })
}

pub(super) fn vertex_location(base_url: &Url) -> Result<String, StatusCode> {
    let host = base_url
        .host_str()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let location = host
        .strip_suffix("-aiplatform.googleapis.com")
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    if location.is_empty() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    Ok(location.to_string())
}

pub(super) fn vertex_version_path(version: GeminiVersion) -> &'static str {
    match version {
        GeminiVersion::V1 => "/v1",
        GeminiVersion::V1Beta => "/v1beta1",
    }
}

pub(super) fn vertex_publisher_model_path(project_id: &str, location: &str, model: &str) -> String {
    let model = model.trim_start_matches('/');
    if model.starts_with("projects/") {
        return model.to_string();
    }
    if model.starts_with("endpoints/") {
        return format!("projects/{project_id}/locations/{location}/{model}");
    }
    if model.starts_with("publishers/") {
        return format!("projects/{project_id}/locations/{location}/{model}");
    }
    if let Some(stripped) = model.strip_prefix("models/") {
        return format!(
            "projects/{project_id}/locations/{location}/publishers/google/models/{stripped}"
        );
    }
    format!("projects/{project_id}/locations/{location}/publishers/google/models/{model}")
}

pub(super) fn vertex_openai_endpoint_path(project_id: &str, location: &str) -> String {
    format!("projects/{project_id}/locations/{location}/endpoints/openapi")
}

pub(super) fn vertex_model_id(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}
