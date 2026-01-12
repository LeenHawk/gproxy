use axum::http::StatusCode;

use crate::context::AppContext;
use crate::providers::geminicli::GeminiCliProvider;

mod claude;
mod gemini;
mod openai;
mod oauth;

pub(crate) use oauth::{
    geminicli_oauth_callback, geminicli_oauth_start,
};

pub(super) async fn get_settings_and_credentials(
    ctx: &AppContext,
) -> Result<GeminiCliProvider, StatusCode> {
    let settings = ctx
        .geminicli()
        .get_config()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let credentials = ctx
        .geminicli()
        .get_credentials()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(GeminiCliProvider {
        setting: settings,
        credentials,
        ..Default::default()
    })
}
