//! Claude Web session-cookie authentication and account bootstrap.

use std::collections::BTreeMap;
use std::sync::Arc;

use bytes::Bytes;
use http::{Request, StatusCode, header};
use serde_json::{Map, Value, json};

use crate::channel::ChannelError;
use crate::http::client::UpstreamClient;

pub(super) const DEFAULT_BASE_URL: &str = "https://claude.ai";
const BOOTSTRAP_ATTEMPTS: u32 = 5;
const VALIDATION_INTERVAL_MS: i64 = 12 * 60 * 60 * 1000;
const COOKIE_ESTIMATED_LIFETIME_MS: i64 = 28 * 24 * 60 * 60 * 1000;
const COOKIE_WARNING_AGE_MS: i64 = 21 * 24 * 60 * 60 * 1000;

pub(super) fn session_key(secret: &Value) -> Result<&str, ChannelError> {
    secret
        .get("cookie")
        .or_else(|| secret.get("session_key"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ChannelError::InvalidCredential("missing Claude sessionKey cookie".into()))
}

pub(super) fn organization_uuid(secret: &Value) -> Result<&str, ChannelError> {
    secret
        .get("account_uuid")
        .or_else(|| secret.get("organization_uuid"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ChannelError::InvalidCredential("missing Claude organization UUID".into()))
}

pub(super) fn is_pro(secret: &Value) -> bool {
    secret
        .get("capabilities")
        .and_then(Value::as_array)
        .is_some_and(|caps| {
            caps.iter().filter_map(Value::as_str).any(|cap| {
                ["pro", "max", "team", "enterprise", "raven"]
                    .iter()
                    .any(|tier| cap.contains(tier))
            })
        })
}

pub(super) fn device_id(secret: &Value) -> Option<&str> {
    secret
        .get("device_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(super) fn cookie_header(session_key: &str) -> String {
    if session_key.trim_start().starts_with("sessionKey=") {
        session_key.trim().to_owned()
    } else {
        format!("sessionKey={}", session_key.trim())
    }
}

pub(super) fn apply_browser_headers(
    req: &mut Request<Bytes>,
    session_key: &str,
    base: &str,
    referer: &str,
) -> Result<(), ChannelError> {
    let headers = req.headers_mut();
    headers.insert(
        header::COOKIE,
        cookie_header(session_key)
            .parse()
            .map_err(|e| ChannelError::Build(format!("claudeweb cookie header: {e}")))?,
    );
    headers.insert(
        header::ORIGIN,
        base.parse()
            .map_err(|e| ChannelError::Build(format!("claudeweb origin header: {e}")))?,
    );
    headers.insert(
        header::REFERER,
        referer
            .parse()
            .map_err(|e| ChannelError::Build(format!("claudeweb referer header: {e}")))?,
    );
    headers.insert(
        header::ACCEPT_LANGUAGE,
        http::HeaderValue::from_static("en-US,en;q=0.9"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        http::HeaderValue::from_static("no-cache"),
    );
    headers.insert(
        "anthropic-client-platform",
        http::HeaderValue::from_static("web_claude_ai"),
    );
    Ok(())
}

pub(super) fn apply_device_header(
    req: &mut Request<Bytes>,
    device_id: Option<&str>,
) -> Result<(), ChannelError> {
    if let Some(device_id) = device_id {
        req.headers_mut().insert(
            "anthropic-device-id",
            device_id
                .parse()
                .map_err(|e| ChannelError::Build(format!("claudeweb device id header: {e}")))?,
        );
        let cookie = req
            .headers()
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok())
            .map(|value| format!("{value}; anthropic-device-id={device_id}"))
            .unwrap_or_else(|| format!("anthropic-device-id={device_id}"));
        req.headers_mut().insert(
            header::COOKIE,
            cookie
                .parse()
                .map_err(|e| ChannelError::Build(format!("claudeweb device cookie: {e}")))?,
        );
    }
    Ok(())
}

/// Validate a pasted claude.ai `sessionKey` and retain the organization data
/// needed by the request path. The cookie itself remains the sole credential.
pub(super) async fn exchange(
    client: &Arc<dyn UpstreamClient>,
    cookie: &str,
) -> Result<Value, ChannelError> {
    let cookie = normalize_cookie(cookie)
        .ok_or_else(|| ChannelError::InvalidCredential("missing sessionKey".into()))?;
    let now_ms = now_ms();
    let mut secret = validate(client, &cookie).await?;
    secret["cookie_received_at_ms"] = Value::from(now_ms);
    secret["cookie_estimated_expires_at_ms"] =
        Value::from(now_ms.saturating_add(COOKIE_ESTIMATED_LIFETIME_MS));
    secret["validated_at_ms"] = Value::from(now_ms);
    secret["device_id"] = Value::String(
        cookie_value(&cookie, "anthropic-device-id")
            .map(str::to_owned)
            .unwrap_or_else(crate::util::rand::uuid_v4),
    );
    Ok(secret)
}

pub(super) fn needs_refresh(secret: &Value) -> bool {
    let Some(validated_at_ms) = secret.get("validated_at_ms").and_then(Value::as_i64) else {
        return true;
    };
    now_ms().saturating_sub(validated_at_ms) >= VALIDATION_INTERVAL_MS
}

/// Revalidate the fixed-lifetime browser cookie and refresh all bootstrap
/// metadata. This cannot extend `sessionKey`; it only detects invalidation and
/// keeps organization/capabilities/models in sync.
pub(super) async fn refresh(
    client: &Arc<dyn UpstreamClient>,
    secret: &Value,
) -> Result<Value, ChannelError> {
    let key = session_key(secret)?.to_owned();
    let now_ms = now_ms();
    let fresh = validate(client, &key).await?;
    let mut merged = secret.as_object().cloned().ok_or_else(|| {
        ChannelError::InvalidCredential("Claude Web secret must be an object".into())
    })?;

    for field in [
        "account_uuid",
        "capabilities",
        "user_email",
        "rate_limit_tier",
        "active_flags",
        "claude_ai_bootstrap_models_config",
        "model_catalog",
    ] {
        merged.remove(field);
    }
    if let Some(fields) = fresh.as_object() {
        merged.extend(fields.clone());
    }

    let received_at_ms = merged
        .get("cookie_received_at_ms")
        .and_then(Value::as_i64)
        .unwrap_or(now_ms);
    merged.insert("cookie_received_at_ms".into(), Value::from(received_at_ms));
    merged
        .entry("cookie_estimated_expires_at_ms")
        .or_insert_with(|| {
            Value::from(received_at_ms.saturating_add(COOKIE_ESTIMATED_LIFETIME_MS))
        });
    merged.insert("validated_at_ms".into(), Value::from(now_ms));
    merged
        .entry("device_id")
        .or_insert_with(|| Value::String(crate::util::rand::uuid_v4()));

    if now_ms.saturating_sub(received_at_ms) >= COOKIE_WARNING_AGE_MS
        && !merged.contains_key("cookie_expiry_warning_at_ms")
    {
        tracing::warn!(
            estimated_expires_at_ms = merged
                .get("cookie_estimated_expires_at_ms")
                .and_then(|value| value.as_i64()),
            "Claude Web sessionKey is at least 21 days old; login will soon be required"
        );
        merged.insert("cookie_expiry_warning_at_ms".into(), Value::from(now_ms));
    }

    Ok(Value::Object(merged))
}

pub(super) fn models(secret: &Value) -> Option<Bytes> {
    let catalog = secret.get("model_catalog")?;
    serde_json::to_vec(catalog).ok().map(Bytes::from)
}

async fn validate(client: &Arc<dyn UpstreamClient>, key: &str) -> Result<Value, ChannelError> {
    let (status, body) = bootstrap(client, key).await?;
    if !status.is_success() {
        let snippet: String = String::from_utf8_lossy(&body).chars().take(256).collect();
        let message = format!("claudeweb bootstrap endpoint {status}: {snippet}");
        return Err(
            if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
                ChannelError::InvalidCredential(message)
            } else {
                ChannelError::Build(message)
            },
        );
    }

    let bootstrap = parse_bootstrap(&body)?;
    let account = bootstrap
        .get("account")
        .and_then(Value::as_object)
        .ok_or_else(|| ChannelError::InvalidCredential("Claude session is not logged in".into()))?;
    let organization = account
        .get("memberships")
        .and_then(Value::as_array)
        .and_then(|memberships| {
            memberships
                .iter()
                .filter_map(|membership| membership.get("organization"))
                .filter(|org| has_capability(org, "chat"))
                .max_by_key(|org| {
                    org.get("capabilities")
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len)
                })
        })
        .ok_or_else(|| {
            ChannelError::Build("claudeweb bootstrap has no chat organization".into())
        })?;
    let account_uuid = organization
        .get("uuid")
        .and_then(Value::as_str)
        .ok_or_else(|| ChannelError::Build("claudeweb organization has no UUID".into()))?;
    let capabilities = organization
        .get("capabilities")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let mut secret = json!({
        "cookie": key,
        "account_uuid": account_uuid,
        "capabilities": capabilities,
    });
    if let Some(email) = account.get("email_address").and_then(Value::as_str) {
        secret["user_email"] = Value::String(email.to_owned());
    }
    if let Some(tier) = rate_limit_tier(organization) {
        secret["rate_limit_tier"] = Value::String(tier.to_owned());
    }
    if let Some(flags) = organization.get("active_flags") {
        secret["active_flags"] = flags.clone();
    }
    if let Some(config) = organization
        .get("claude_ai_bootstrap_models_config")
        .filter(|value| !value.is_null())
    {
        secret["claude_ai_bootstrap_models_config"] = config.clone();
        if let Some(catalog) = model_catalog(config) {
            secret["model_catalog"] = catalog;
        }
    }
    Ok(secret)
}

fn now_ms() -> i64 {
    crate::util::time::unix_now().saturating_mul(1000)
}

fn model_catalog(config: &Value) -> Option<Value> {
    let mut models = BTreeMap::<String, Option<String>>::new();
    collect_models(config, &mut models);
    if models.is_empty() {
        return None;
    }
    let data = models
        .into_iter()
        .map(|(id, display_name)| {
            let mut model = Map::from_iter([("id".into(), Value::String(id))]);
            if let Some(display_name) = display_name {
                model.insert("display_name".into(), Value::String(display_name));
            }
            Value::Object(model)
        })
        .collect::<Vec<_>>();
    let first_id = data.first().and_then(|m| m.get("id")).cloned();
    let last_id = data.last().and_then(|m| m.get("id")).cloned();
    Some(json!({
        "data": data,
        "first_id": first_id,
        "last_id": last_id,
        "has_more": false,
    }))
}

fn collect_models(value: &Value, models: &mut BTreeMap<String, Option<String>>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_models(item, models);
            }
        }
        Value::Object(object) => {
            let id = ["id", "model", "model_id", "value"]
                .into_iter()
                .find_map(|key| object.get(key).and_then(Value::as_str))
                .filter(|id| is_claude_model_id(id));
            let display_name = ["display_name", "displayName", "label", "name"]
                .into_iter()
                .find_map(|key| object.get(key).and_then(Value::as_str))
                .filter(|name| !is_claude_model_id(name))
                .map(str::to_owned);
            if let Some(id) = id {
                models
                    .entry(id.to_owned())
                    .and_modify(|current| {
                        if current.is_none() {
                            *current = display_name.clone();
                        }
                    })
                    .or_insert(display_name);
            }
            for (key, child) in object {
                if is_claude_model_id(key) {
                    let display = child
                        .as_object()
                        .and_then(|o| {
                            ["display_name", "displayName", "label", "name"]
                                .into_iter()
                                .find_map(|field| o.get(field).and_then(Value::as_str))
                        })
                        .filter(|name| !is_claude_model_id(name))
                        .map(str::to_owned);
                    models.entry(key.clone()).or_insert(display);
                }
                collect_models(child, models);
            }
        }
        Value::String(text) => {
            if let Ok(nested) = serde_json::from_str::<Value>(text) {
                collect_models(&nested, models);
            }
        }
        _ => {}
    }
}

fn is_claude_model_id(value: &str) -> bool {
    value.starts_with("claude-") && value.len() > "claude-".len()
}

async fn bootstrap(
    client: &Arc<dyn UpstreamClient>,
    key: &str,
) -> Result<(http::StatusCode, Bytes), ChannelError> {
    let mut last = None;
    for attempt in 0..BOOTSTRAP_ATTEMPTS {
        let url = format!("{DEFAULT_BASE_URL}/api/bootstrap");
        let mut req = Request::get(url)
            .header(header::ACCEPT, "application/json")
            .body(Bytes::new())
            .map_err(|e| ChannelError::Build(format!("claudeweb bootstrap request: {e}")))?;
        apply_browser_headers(
            &mut req,
            key,
            DEFAULT_BASE_URL,
            &format!("{DEFAULT_BASE_URL}/new"),
        )?;
        let response = client
            .send(req)
            .await
            .map_err(|e| ChannelError::Build(format!("claudeweb bootstrap failed: {e}")))?;
        let (parts, body) = response.into_parts();
        if parts.status.is_success() || !is_cloudflare_challenge(parts.status, &body) {
            return Ok((parts.status, body));
        }
        last = Some((parts.status, body));
        crate::util::time::sleep_ms(200 * u64::from(attempt + 1)).await;
    }
    last.ok_or_else(|| ChannelError::Build("claudeweb bootstrap produced no response".into()))
}

fn is_cloudflare_challenge(status: http::StatusCode, body: &[u8]) -> bool {
    if !matches!(status.as_u16(), 403 | 429 | 503) {
        return false;
    }
    let text = String::from_utf8_lossy(body).to_ascii_lowercase();
    text.contains("just a moment")
        || text.contains("challenge-platform")
        || text.contains("cf-chl-")
        || text.contains("cloudflare")
}

fn has_capability(organization: &Value, needle: &str) -> bool {
    organization
        .get("capabilities")
        .and_then(Value::as_array)
        .is_some_and(|caps| caps.iter().any(|cap| cap.as_str() == Some(needle)))
}

fn rate_limit_tier(organization: &Value) -> Option<&str> {
    if let Some(tier) = organization
        .get("rate_limit_tier")
        .and_then(Value::as_str)
        .filter(|tier| !tier.trim().is_empty())
    {
        return Some(tier);
    }
    let caps = organization.get("capabilities")?.as_array()?;
    let has = |needle: &str| {
        caps.iter()
            .filter_map(Value::as_str)
            .any(|cap| cap.contains(needle))
    };
    if has("max") {
        Some("max")
    } else if has("enterprise") {
        Some("enterprise")
    } else if has("team") {
        Some("team")
    } else if has("pro") {
        Some("pro")
    } else {
        None
    }
}

fn parse_bootstrap(body: &[u8]) -> Result<Value, ChannelError> {
    for value in serde_json::Deserializer::from_slice(body)
        .into_iter::<Value>()
        .flatten()
    {
        if value.get("account").is_some() {
            return Ok(value);
        }
    }
    Err(ChannelError::Build(
        "claudeweb bootstrap response is not valid account JSON".into(),
    ))
}

pub(super) fn normalize_session_key(input: &str) -> Option<String> {
    let mut text = input.trim();
    if let Some((name, value)) = text.split_once(':')
        && name.trim().eq_ignore_ascii_case("cookie")
    {
        text = value.trim();
    }
    for part in text.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("sessionKey=") {
            let value = value.trim();
            if value.starts_with("sk-ant-sid") {
                return Some(value.to_owned());
            }
        }
    }
    (text.starts_with("sk-ant-sid") && !text.contains(['=', ';'])).then(|| text.to_owned())
}

/// Preserve the complete browser Cookie header when one is pasted. Claude's
/// `sessionKey` authenticates the account, but Cloudflare may additionally
/// require browser-issued cookies such as `cf_clearance` and `__cf_bm`.
fn normalize_cookie(input: &str) -> Option<String> {
    let mut text = input.trim();
    if let Some((name, value)) = text.split_once(':')
        && name.trim().eq_ignore_ascii_case("cookie")
    {
        text = value.trim();
    }
    let session_key = normalize_session_key(text)?;
    if !text.contains("sessionKey=") {
        return Some(session_key);
    }
    let pairs = text
        .split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty() && part.contains('='))
        .collect::<Vec<_>>();
    (!pairs.is_empty()).then(|| pairs.join("; "))
}

fn cookie_value<'a>(cookie: &'a str, name: &str) -> Option<&'a str> {
    cookie.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        key.eq_ignore_ascii_case(name).then_some(value.trim())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_cookie_header_and_bare_value() {
        assert_eq!(
            normalize_session_key("foo=bar; sessionKey=sk-ant-sid01-example; x=y").as_deref(),
            Some("sk-ant-sid01-example")
        );
        assert_eq!(
            normalize_session_key("sk-ant-sid02-example").as_deref(),
            Some("sk-ant-sid02-example")
        );
    }

    #[test]
    fn preserves_full_browser_cookie_and_device_id() {
        let cookie = normalize_cookie(
            "Cookie: cf_clearance=clear; sessionKey=sk-ant-sid01-example; anthropic-device-id=device-1",
        )
        .unwrap();
        assert_eq!(
            cookie,
            "cf_clearance=clear; sessionKey=sk-ant-sid01-example; anthropic-device-id=device-1"
        );
        assert_eq!(
            cookie_value(&cookie, "anthropic-device-id"),
            Some("device-1")
        );
        assert_eq!(
            normalize_cookie("sk-ant-sid02-example").as_deref(),
            Some("sk-ant-sid02-example")
        );
    }

    #[test]
    fn refreshes_missing_or_stale_validation_only() {
        let now = now_ms();
        assert!(needs_refresh(&json!({"cookie": "sk-ant-sid-example"})));
        assert!(!needs_refresh(&json!({
            "validated_at_ms": now - VALIDATION_INTERVAL_MS + 1_000
        })));
        assert!(needs_refresh(&json!({
            "validated_at_ms": now - VALIDATION_INTERVAL_MS - 1_000
        })));
    }

    #[test]
    fn extracts_models_from_array_and_keyed_configs() {
        let catalog = model_catalog(&json!({
            "models": [
                {"model": "claude-sonnet-5", "display_name": "Claude Sonnet 5"},
                {"id": "claude-fable-5", "label": "Claude Fable 5"}
            ],
            "claude-opus-4-8": {"name": "Claude Opus 4.8"},
            "unrelated": {"id": "not-a-claude-model"}
        }))
        .unwrap();
        let data = catalog["data"].as_array().unwrap();
        assert_eq!(data.len(), 3);
        assert!(data.iter().any(|model| {
            model["id"] == "claude-sonnet-5" && model["display_name"] == "Claude Sonnet 5"
        }));
        assert!(data.iter().any(|model| {
            model["id"] == "claude-opus-4-8" && model["display_name"] == "Claude Opus 4.8"
        }));
    }
}
