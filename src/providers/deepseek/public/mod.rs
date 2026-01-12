use axum::http::StatusCode;

use crate::context::AppContext;
use crate::providers::deepseek::DeepSeekProvider;

mod claude;
mod gemini;
mod openai;

pub(super) async fn get_settings_and_credentials(
    ctx: &AppContext,
) -> Result<DeepSeekProvider, StatusCode> {
    let settings = ctx
        .deepseek()
        .get_config()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let credentials = ctx
        .deepseek()
        .get_credentials()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(DeepSeekProvider {
        setting: settings,
        credentials,
        ..Default::default()
    })
}
