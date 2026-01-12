use axum::http::StatusCode;

use crate::context::AppContext;
use crate::providers::vertexexpress::VertexExpressProvider;

mod claude;
mod gemini;
mod openai;

pub(super) async fn get_settings_and_credentials(
    ctx: &AppContext,
) -> Result<VertexExpressProvider, StatusCode> {
    let settings = ctx
        .vertexexpress()
        .get_config()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let credentials = ctx
        .vertexexpress()
        .get_credentials()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(VertexExpressProvider {
        setting: settings,
        credentials,
        ..Default::default()
    })
}
