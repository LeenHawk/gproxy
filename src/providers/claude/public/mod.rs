use axum::http::{HeaderMap, HeaderValue, StatusCode};

use crate::context::AppContext;
use crate::providers::claude::ClaudeProvider;

mod claude;
mod gemini;
mod openai;

pub(super) async fn get_settings_and_credentials(
    ctx: &AppContext,
) -> Result<ClaudeProvider, StatusCode> {
    let settings = ctx
        .claude()
        .get_config()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let credentials = ctx
        .claude()
        .get_credentials()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(ClaudeProvider {
        setting: settings,
        credentials,
        ..Default::default()
    })
}

pub(super) fn anthropic_version(headers: &HeaderMap) -> Result<HeaderValue, StatusCode> {
    headers
        .get("anthropic-version")
        .cloned()
        .ok_or(StatusCode::BAD_REQUEST)
}

pub(super) fn ensure_skills_beta(headers: &mut HeaderMap) {
    const SKILLS_BETA: &str = "skills-2025-10-02";
    let mut has_beta = false;
    for value in headers.get_all("anthropic-beta").iter() {
        if let Ok(value) = value.to_str()
            && value.split(',').any(|part| part.trim() == SKILLS_BETA) {
                has_beta = true;
                break;
            }
    }
    if !has_beta {
        headers.append("anthropic-beta", HeaderValue::from_static(SKILLS_BETA));
    }
}

pub(super) fn ensure_files_beta(headers: &mut HeaderMap) -> Result<(), StatusCode> {
    const HEADER_NAME: &str = "anthropic-beta";
    const FILES_BETA: &str = "files-api-2025-04-14";

    let mut has_beta = false;
    for value in headers.get_all(HEADER_NAME).iter() {
        if let Ok(value) = value.to_str()
            && value
                .split(',')
                .map(|item| item.trim())
                .any(|item| item == FILES_BETA)
            {
                has_beta = true;
                break;
            }
    }

    if !has_beta {
        headers.append(HEADER_NAME, HeaderValue::from_static(FILES_BETA));
    }

    Ok(())
}
