use axum::http::StatusCode;

use crate::context::AppContext;
use crate::providers::openai::OpenAIProvider;

mod claude;
mod gemini;
mod openai;

pub(super) async fn get_settings_and_credentials(
    ctx: &AppContext,
) -> Result<OpenAIProvider, StatusCode> {
    let settings = ctx
        .openai()
        .get_config()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let credentials = ctx
        .openai()
        .get_credentials()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(OpenAIProvider {
        setting: settings,
        credentials,
        ..Default::default()
    })
}
