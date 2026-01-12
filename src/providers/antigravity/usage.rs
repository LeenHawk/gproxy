use std::collections::HashMap;

use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use serde::Serialize;
use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use crate::context::AppContext;
use crate::providers::antigravity::constants::ANTIGRAVITY_USER_AGENT;
use crate::providers::antigravity::AntigravityCredential;
use crate::providers::credential_status::{CredentialStatus, CredentialStatusList, now_timestamp};
use crate::providers::credential_status::DEFAULT_MODEL_KEY;
use crate::providers::credential_status::ProviderKind;
use crate::providers::router::{
    AuthMode, ParsedBody, build_url, parse_json_response, send_json_request_with_status,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AntigravityUsage {
    pub models: HashMap<String, AntigravityQuotaInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AntigravityQuotaInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_fraction: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_time: Option<String>,
}

pub(crate) async fn fetch_antigravity_usage(
    ctx: &AppContext,
    credential: &AntigravityCredential,
) -> Result<AntigravityUsage, StatusCode> {
    let setting = ctx
        .antigravity()
        .get_config()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let access_token = ensure_access_token(ctx, credential).await?;
    let url = build_url(&setting.base_url, "v1internal:fetchAvailableModels")?;
    let res = send_json_request_with_status(
        ctx,
        ProviderKind::Antigravity,
        credential.project_id.as_str(),
        DEFAULT_MODEL_KEY,
        ctx.http_client(),
        url.as_str(),
        &HeaderMap::new(),
        AuthMode::AuthorizationBearer,
        access_token.as_str(),
        |headers| {
            apply_antigravity_usage_headers(headers)?;
            Ok(())
        },
        &serde_json::json!({}),
    )
    .await?;
    let parsed = parse_json_response::<Value>(res).await?;
    let value = match parsed.body {
        ParsedBody::Ok(value) => value,
        ParsedBody::Error(_) => return Err(StatusCode::BAD_GATEWAY),
    };
    Ok(parse_usage_from_value(value))
}

pub(crate) fn apply_usage_to_states(
    states: &mut CredentialStatusList,
    usage: &AntigravityUsage,
) -> bool {
    let before = states.clone();
    let now = now_timestamp();
    for (model, quota) in &usage.models {
        let Some(remaining) = quota.remaining_fraction else {
            continue;
        };
        if remaining <= 0.0 {
            let Some(reset_time) = quota.reset_time.as_deref() else {
                continue;
            };
            let Some(until) = parse_reset_time(reset_time) else {
                continue;
            };
            if until > now {
                states.update_status(model, CredentialStatus::Cooldown { until });
            } else {
                states.update_status(model, CredentialStatus::Active);
            }
        } else {
            states.update_status(model, CredentialStatus::Active);
        }
    }
    *states != before
}

fn parse_usage_from_value(value: Value) -> AntigravityUsage {
    let mut models = HashMap::new();
    let Some(model_map) = value.get("models").and_then(|item| item.as_object()) else {
        return AntigravityUsage { models };
    };
    for (model_id, model_value) in model_map {
        let Some(quota) = model_value
            .get("quotaInfo")
            .and_then(|item| item.as_object())
        else {
            continue;
        };
        let remaining = quota
            .get("remainingFraction")
            .and_then(|item| item.as_f64())
            .or_else(|| quota.get("remainingFraction").and_then(|item| item.as_i64()).map(|v| v as f64));
        let reset_time = quota
            .get("resetTime")
            .and_then(|item| item.as_str())
            .map(|value| value.to_string());
        if remaining.is_none() && reset_time.is_none() {
            continue;
        }
        let normalized = normalize_model_key(model_id);
        models.insert(
            normalized,
            AntigravityQuotaInfo {
                remaining_fraction: remaining,
                reset_time,
            },
        );
    }
    AntigravityUsage { models }
}

fn normalize_model_key(model_id: &str) -> String {
    let trimmed = model_id.trim().trim_start_matches('/');
    if let Some(stripped) = trimmed.strip_prefix("models/") {
        stripped.to_string()
    } else {
        trimmed.to_string()
    }
}

fn parse_reset_time(value: &str) -> Option<i64> {
    OffsetDateTime::parse(value, &Rfc3339)
        .ok()
        .map(|dt| dt.unix_timestamp())
}

fn apply_antigravity_usage_headers(headers: &mut HeaderMap) -> Result<(), StatusCode> {
    headers.insert(
        header::USER_AGENT,
        HeaderValue::from_static(ANTIGRAVITY_USER_AGENT),
    );
    headers.insert(
        header::ACCEPT_ENCODING,
        HeaderValue::from_static("gzip"),
    );
    let request_id = format!("req-{}", Uuid::new_v4());
    let request_id =
        HeaderValue::from_str(&request_id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    headers.insert("requestId", request_id);
    Ok(())
}

// impl_google_access_token! expands to ensure_access_token(...) for Google OAuth credentials.
crate::impl_google_access_token!(crate::providers::antigravity::AntigravityCredential, antigravity);
