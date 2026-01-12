use axum::http::StatusCode;

use crate::context::AppContext;
use crate::providers::aistudio::AIStudioProvider;

mod claude;
mod gemini;
mod openai;

pub(super) async fn get_settings_and_credentials(
    ctx: &AppContext,
) -> Result<AIStudioProvider, StatusCode> {
    let settings = ctx
        .aistudio()
        .get_config()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let credentials = ctx
        .aistudio()
        .get_credentials()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(AIStudioProvider {
        setting: settings,
        credentials,
        ..Default::default()
    })
}
