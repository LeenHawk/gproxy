use axum::http::StatusCode;
use serde_json::Value;
use tokio::time::{Duration, sleep};
use url::Url;

use crate::context::AppContext;

const LOAD_CODE_ASSIST_PATH: &str = "v1internal:loadCodeAssist";
const ONBOARD_USER_PATH: &str = "v1internal:onboardUser";
const LOAD_CODE_ASSIST_METADATA: &str = r#"{"metadata":{"ideType":"ANTIGRAVITY","platform":"PLATFORM_UNSPECIFIED","pluginType":"GEMINI"}}"#;

pub(crate) async fn fetch_project_id(
    ctx: &AppContext,
    base_url: &Url,
    access_token: &str,
    user_agent: &str,
) -> Result<Option<String>, StatusCode> {
    if let Some(project_id) = try_load_code_assist(ctx, base_url, access_token, user_agent).await? {
        return Ok(Some(project_id));
    }
    try_onboard_user(ctx, base_url, access_token, user_agent).await
}

async fn try_load_code_assist(
    ctx: &AppContext,
    base_url: &Url,
    access_token: &str,
    user_agent: &str,
) -> Result<Option<String>, StatusCode> {
    let url = base_url
        .join(LOAD_CODE_ASSIST_PATH)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let Some(value) = post_json(ctx, url.as_str(), access_token, user_agent, LOAD_CODE_ASSIST_METADATA).await? else {
        return Ok(None);
    };
    let current_tier = value.get("currentTier");
    if current_tier.is_none() {
        return Ok(None);
    }
    let project = value.get("cloudaicompanionProject");
    if let Some(project_id) = project.and_then(|value| value.as_str()) {
        return Ok(Some(project_id.to_string()));
    }
    if let Some(project_id) = project
        .and_then(|value| value.get("id"))
        .and_then(|value| value.as_str())
    {
        return Ok(Some(project_id.to_string()));
    }
    Ok(None)
}

async fn try_onboard_user(
    ctx: &AppContext,
    base_url: &Url,
    access_token: &str,
    user_agent: &str,
) -> Result<Option<String>, StatusCode> {
    let Some(tier_id) = get_onboard_tier(ctx, base_url, access_token, user_agent).await? else {
        return Ok(None);
    };
    let url = base_url
        .join(ONBOARD_USER_PATH)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let body = format!(
        "{{\"tierId\":\"{tier}\",\"metadata\":{{\"ideType\":\"ANTIGRAVITY\",\"platform\":\"PLATFORM_UNSPECIFIED\",\"pluginType\":\"GEMINI\"}}}}",
        tier = tier_id
    );

    let max_attempts = 5;
    for _ in 0..max_attempts {
        let Some(value) = post_json(ctx, url.as_str(), access_token, user_agent, &body).await? else {
            return Ok(None);
        };
        let done = value.get("done").and_then(|value| value.as_bool()).unwrap_or(false);
        if done {
            return Ok(extract_project_id_from_onboard(&value));
        }
        sleep(Duration::from_secs(2)).await;
    }
    Ok(None)
}

async fn get_onboard_tier(
    ctx: &AppContext,
    base_url: &Url,
    access_token: &str,
    user_agent: &str,
) -> Result<Option<String>, StatusCode> {
    let url = base_url
        .join(LOAD_CODE_ASSIST_PATH)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let Some(value) = post_json(ctx, url.as_str(), access_token, user_agent, LOAD_CODE_ASSIST_METADATA).await? else {
        return Ok(None);
    };
    let allowed = value.get("allowedTiers").and_then(|value| value.as_array());
    if let Some(allowed) = allowed {
        for tier in allowed {
            if tier.get("isDefault").and_then(|value| value.as_bool()).unwrap_or(false)
                && let Some(tier_id) = tier.get("id").and_then(|value| value.as_str())
            {
                return Ok(Some(tier_id.to_string()));
            }
        }
    }
    Ok(Some("LEGACY".to_string()))
}

fn extract_project_id_from_onboard(value: &Value) -> Option<String> {
    let response = value.get("response")?;
    let project = response.get("cloudaicompanionProject")?;
    if let Some(project_id) = project.as_str() {
        return Some(project_id.to_string());
    }
    project.get("id")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

async fn post_json(
    ctx: &AppContext,
    url: &str,
    access_token: &str,
    user_agent: &str,
    body: &str,
) -> Result<Option<Value>, StatusCode> {
    let res = ctx
        .http_client()
        .post(url)
        .header("Authorization", format!("Bearer {access_token}"))
        .header("User-Agent", user_agent)
        .header("Content-Type", "application/json")
        .header("Accept-Encoding", "gzip")
        .body(body.to_string())
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    if !res.status().is_success() {
        return Ok(None);
    }

    let bytes = res.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| StatusCode::BAD_GATEWAY)?;
    Ok(Some(value))
}
