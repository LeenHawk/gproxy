use std::sync::Arc;

use axum::extract::Extension;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;

use crate::context::AppContext;
use crate::providers::antigravity::AntigravityProvider;
use crate::providers::antigravity::{AntigravityUsage, apply_usage_to_states, fetch_antigravity_usage};
use crate::providers::auth::{ApiFormat, ensure_public_auth};

mod claude;
mod gemini;
mod openai;
mod oauth;

pub(crate) use oauth::{
    antigravity_oauth_callback, antigravity_oauth_start,
};

pub(crate) async fn antigravity_usage(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
) -> Result<Json<AntigravityUsage>, StatusCode> {
    let query = std::collections::HashMap::new();
    let _caller_api_key = ensure_public_auth(ApiFormat::Gemini, &headers, &query, ctx.as_ref())?;
    let provider = get_settings_and_credentials(ctx.as_ref()).await?;
    let credential = provider
        .pick_credential()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let usage = fetch_antigravity_usage(ctx.as_ref(), credential).await?;
    let mut next_states = credential.states.clone();
    if apply_usage_to_states(&mut next_states, &usage) {
        let project_key = credential.project_id.clone();
        ctx.antigravity()
            .update_credential_by_id(&project_key, move |stored| {
                stored.states = next_states;
            })
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    Ok(Json(usage))
}

pub(super) async fn get_settings_and_credentials(
    ctx: &AppContext,
) -> Result<AntigravityProvider, StatusCode> {
    let settings = ctx
        .antigravity()
        .get_config()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let credentials = ctx
        .antigravity()
        .get_credentials()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(AntigravityProvider {
        setting: settings,
        credentials,
        ..Default::default()
    })
}
