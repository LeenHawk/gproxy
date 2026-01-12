use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::Extension;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use time::OffsetDateTime;

use crate::context::AppContext;
use crate::providers::auth::{ApiFormat, ensure_public_auth};
use crate::providers::claudecode::{
    ClaudeCodeProvider, CLAUDE_BETA_BASE, CLAUDE_CODE_USER_AGENT,
};
use crate::providers::credential_status::{CredentialStatus, DEFAULT_MODEL_KEY, ProviderKind};
use crate::providers::router::{
    AuthMode, ParsedBody, parse_json_response, render_json_response, send_get_request_with_status,
};

mod claude;
mod gemini;
mod openai;
mod oauth;

pub(crate) use oauth::{
    claudecode_oauth_callback, claudecode_oauth_start,
};

pub(crate) async fn claudecode_usage(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let query = HashMap::new();
    let _caller_api_key = ensure_public_auth(ApiFormat::Claude, &headers, &query, ctx.as_ref())?;
    let provider = get_settings_and_credentials(ctx.as_ref()).await?;
    let credential = provider
        .pick_credential()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut url = provider.setting.base_url.clone();
    url.set_path("/api/oauth/usage");
    url.set_query(None);
    let res = send_get_request_with_status(
        ctx.as_ref(),
        ProviderKind::ClaudeCode,
        credential.refresh_token.as_str(),
        crate::providers::credential_status::DEFAULT_MODEL_KEY,
        ctx.http_client(),
        url.as_str(),
        &headers,
        AuthMode::AuthorizationBearer,
        credential.access_token.as_str(),
        |out_headers| {
            out_headers.remove("anthropic-version");
            out_headers.insert(
                header::ACCEPT,
                HeaderValue::from_static("application/json, text/plain, */*"),
            );
            out_headers.insert(
                header::USER_AGENT,
                HeaderValue::from_static(CLAUDE_CODE_USER_AGENT),
            );
            out_headers.insert(
                "anthropic-beta",
                HeaderValue::from_static(CLAUDE_BETA_BASE),
            );
            Ok(())
        },
    )
    .await?;
    let parsed = parse_json_response::<serde_json::Value>(res).await?;
    if let ParsedBody::Ok(ref body) = parsed.body {
        update_usage_anchors(ctx.as_ref(), credential.refresh_token.as_str(), body).await;
    }
    render_json_response(parsed)
}

pub(super) async fn get_settings_and_credentials(
    ctx: &AppContext,
) -> Result<ClaudeCodeProvider, StatusCode> {
    let settings = ctx
        .claudecode()
        .get_config()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let credentials = ctx
        .claudecode()
        .get_credentials()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(ClaudeCodeProvider {
        setting: settings,
        credentials,
        ..Default::default()
    })
}

async fn update_usage_anchors(
    ctx: &AppContext,
    credential_id: &str,
    usage: &serde_json::Value,
) {
    let window_specs = [
        ("5h", "five_hour", 5 * 60 * 60, false),
        ("1w", "seven_day", 7 * 24 * 60 * 60, false),
        ("1w_sonnet", "seven_day_sonnet", 7 * 24 * 60 * 60, true),
    ];
    let mut cooldown_until: Option<i64> = None;
    let mut sonnet_cooldown_until: Option<i64> = None;

    for (view_name, usage_key, slot_secs, is_sonnet_only) in window_specs {
        let reset_at = parse_reset_at(usage, usage_key);
        if let Some(reset_at) = reset_at {
            let anchor_ts = reset_at - slot_secs;
            let _ = ctx
                .usage_store()
                .set_anchor(ProviderKind::ClaudeCode, view_name, anchor_ts)
                .await;
            ctx.schedule_usage_anchor(ProviderKind::ClaudeCode, view_name, reset_at)
                .await;
        }

        let utilization = parse_utilization(usage, usage_key);
        if utilization.is_some_and(|value| value >= 100.0)
            && let Some(reset_at) = reset_at
        {
            if is_sonnet_only {
                sonnet_cooldown_until = Some(
                    sonnet_cooldown_until.map_or(reset_at, |prev| prev.max(reset_at)),
                );
            } else {
                cooldown_until = Some(cooldown_until.map_or(reset_at, |prev| prev.max(reset_at)));
            }
        }
    }

    if let Some(until) = cooldown_until {
        let _ = ctx
            .update_credential_status_by_id(
                ProviderKind::ClaudeCode,
                credential_id,
                DEFAULT_MODEL_KEY,
                |prev, now| {
                    if matches!(prev, CredentialStatus::Disabled) {
                        return None;
                    }
                    if until > now {
                        return Some(CredentialStatus::Cooldown { until });
                    }
                    Some(CredentialStatus::Active)
                },
            )
            .await;
    }
    if let Some(until) = sonnet_cooldown_until {
        let _ = ctx
            .update_credential_status_by_id(
                ProviderKind::ClaudeCode,
                credential_id,
                "sonnet",
                |prev, now| {
                    if matches!(prev, CredentialStatus::Disabled) {
                        return None;
                    }
                    if until > now {
                        return Some(CredentialStatus::Cooldown { until });
                    }
                    Some(CredentialStatus::Active)
                },
            )
            .await;
    }
}

fn parse_reset_at(usage: &serde_json::Value, key: &str) -> Option<i64> {
    usage
        .get(key)
        .and_then(|value| value.get("resets_at"))
        .and_then(|value| value.as_str())
        .and_then(|value| {
            OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
                .ok()
        })
        .map(|dt| dt.unix_timestamp())
}

fn parse_utilization(usage: &serde_json::Value, key: &str) -> Option<f64> {
    usage
        .get(key)
        .and_then(|value| value.get("utilization"))
        .and_then(|value| value.as_f64())
}
