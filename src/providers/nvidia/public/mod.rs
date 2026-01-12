use axum::http::StatusCode;

use crate::context::AppContext;
use crate::providers::nvidia::NvidiaProvider;

mod claude;
mod gemini;
mod openai;

pub(super) async fn get_settings_and_credentials(
    ctx: &AppContext,
) -> Result<NvidiaProvider, StatusCode> {
    let settings = ctx
        .nvidia()
        .get_config()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let credentials = ctx
        .nvidia()
        .get_credentials()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(NvidiaProvider {
        setting: settings,
        credentials,
        ..Default::default()
    })
}
