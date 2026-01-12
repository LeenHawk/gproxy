use std::collections::{HashMap, HashSet};

use axum::http::{HeaderMap, StatusCode};

use crate::context::AppContext;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApiFormat {
    OpenAI,
    Claude,
    Gemini,
}

pub fn format_from_request(
    headers: &HeaderMap,
    query: &HashMap<String, String>,
) -> Result<ApiFormat, StatusCode> {
    let mut formats = HashSet::new();

    if bearer_token(headers)
        .filter(|token| !token.is_empty())
        .is_some()
    {
        formats.insert(ApiFormat::OpenAI);
    }

    let has_anthropic_version = headers
        .get("anthropic-version")
        .and_then(|value| value.to_str().ok())
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    if has_anthropic_version
        && header_value(headers, "x-api-key")
            .filter(|key| !key.is_empty())
            .is_some()
    {
        formats.insert(ApiFormat::Claude);
    }

    let has_gemini_key = header_value(headers, "x-goog-api-key")
        .filter(|key| !key.is_empty())
        .is_some()
        || query
            .get("key")
            .map(|key| !key.trim().is_empty())
            .unwrap_or(false);
    if has_gemini_key {
        formats.insert(ApiFormat::Gemini);
    }

    if formats.len() == 1 {
        Ok(*formats.iter().next().expect("non-empty formats"))
    } else {
        Err(StatusCode::BAD_REQUEST)
    }
}

pub fn ensure_public_auth(
    format: ApiFormat,
    headers: &HeaderMap,
    query: &HashMap<String, String>,
    ctx: &AppContext,
) -> Result<String, StatusCode> {
    let key = match format {
        ApiFormat::OpenAI => bearer_token(headers),
        ApiFormat::Claude => {
            let version = headers
                .get("anthropic-version")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default();
            if version.is_empty() {
                return Err(StatusCode::BAD_REQUEST);
            }
            header_value(headers, "x-api-key")
        }
        ApiFormat::Gemini => {
            header_value(headers, "x-goog-api-key").or_else(|| query.get("key").cloned())
        }
    }
    .ok_or(StatusCode::BAD_REQUEST)?;

    let app = ctx.get_config();
    if key == app.app.admin_key || app.app.api_keys.iter().any(|item| item == &key) {
        return Ok(key);
    }

    Err(StatusCode::UNAUTHORIZED)
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = header_value(headers, "authorization")?;
    let value = value.trim();
    value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .map(|token| token.trim().to_string())
}

fn header_value(headers: &HeaderMap, name: &'static str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string())
}
