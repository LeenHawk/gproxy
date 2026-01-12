use std::sync::Arc;

use axum::extract::Extension;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;

use crate::context::AppContext;
use crate::providers::auth::{ApiFormat, ensure_public_auth};
use crate::providers::credential_status::ProviderKind;
use crate::providers::router::{
    AuthMode, parse_json_response, render_json_response, send_get_request_with_status,
};
use crate::providers::codex::CodexProvider;

mod claude;
mod gemini;
mod openai;
mod oauth;

pub(crate) use oauth::{
    codex_oauth_callback, codex_oauth_start,
};

pub(crate) async fn codex_usage(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let query = std::collections::HashMap::new();
    let _caller_api_key = ensure_public_auth(ApiFormat::OpenAI, &headers, &query, ctx.as_ref())?;
    let provider = get_settings_and_credentials(ctx.as_ref()).await?;
    let credential = provider
        .pick_credential()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut url = provider.setting.base_url.clone();
    url.set_path("/backend-api/wham/usage");
    url.set_query(None);
    let res = send_get_request_with_status(
        ctx.as_ref(),
        ProviderKind::Codex,
        credential.account_id.as_str(),
        crate::providers::credential_status::DEFAULT_MODEL_KEY,
        ctx.http_client(),
        url.as_str(),
        &headers,
        AuthMode::AuthorizationBearer,
        credential.access_token.as_str(),
        |headers| {
            openai::apply_codex_headers(headers, credential.account_id.as_str(), false)?;
            Ok(())
        },
    )
    .await?;
    let parsed = parse_json_response::<serde_json::Value>(res).await?;
    render_json_response(parsed)
}

pub(super) async fn get_settings_and_credentials(
    ctx: &AppContext,
) -> Result<CodexProvider, StatusCode> {
    let settings = ctx
        .codex()
        .get_config()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let credentials = ctx
        .codex()
        .get_credentials()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(CodexProvider {
        setting: settings,
        credentials,
        ..Default::default()
    })
}
