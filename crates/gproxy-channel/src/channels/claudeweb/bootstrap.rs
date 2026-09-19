//! `GET /api/bootstrap`: the logged-in account, its organizations and the
//! chat organization's capabilities and model configuration
//! (v3 `claudeweb/bootstrap.rs`). The same reply serves cookie login and
//! the twelve-hourly re-validation.

use serde_json::{Map, Value, json};

use super::models;

pub(super) struct Bootstrap {
    pub organization: String,
    pub capabilities: Value,
    pub active_flags: Option<Value>,
    pub rate_limit_tier: Option<Value>,
    pub email: Option<String>,
    /// `{id, display_name}` pairs the front end may pick from.
    pub models: Vec<models::Model>,
}

pub(super) enum BootstrapFailure {
    /// The reply carries no `account`: the session is not logged in.
    LoggedOut,
    Malformed(String),
}

/// Pick the chat organization with the richest capability set, as v3 did
/// (`samples/clewdr` takes the first one with `chat`).
pub(super) fn parse(body: &[u8]) -> Result<Bootstrap, BootstrapFailure> {
    let bootstrap = serde_json::Deserializer::from_slice(body)
        .into_iter::<Value>()
        .filter_map(Result::ok)
        .find(|value| value.get("account").is_some())
        .ok_or_else(|| BootstrapFailure::Malformed("bootstrap account JSON missing".into()))?;
    let account = bootstrap
        .get("account")
        .and_then(Value::as_object)
        .ok_or(BootstrapFailure::LoggedOut)?;
    let organization = account
        .get("memberships")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|membership| membership.get("organization"))
        .filter(|organization| has_capability(organization, "chat"))
        .max_by_key(|organization| {
            organization
                .get("capabilities")
                .and_then(Value::as_array)
                .map_or(0, Vec::len)
        })
        .ok_or_else(|| BootstrapFailure::Malformed("bootstrap has no chat organization".into()))?;
    let uuid = organization
        .get("uuid")
        .and_then(Value::as_str)
        .ok_or_else(|| BootstrapFailure::Malformed("chat organization UUID missing".into()))?;
    Ok(Bootstrap {
        organization: uuid.to_owned(),
        capabilities: organization
            .get("capabilities")
            .cloned()
            .unwrap_or_else(|| json!([])),
        active_flags: organization.get("active_flags").cloned(),
        rate_limit_tier: organization
            .get("rate_limit_tier")
            .or_else(|| organization.get("rateLimitTier"))
            .cloned(),
        email: account
            .get("email_address")
            .and_then(Value::as_str)
            .map(str::to_owned),
        models: organization
            .get("claude_ai_bootstrap_models_config")
            .map(models::from_config)
            .unwrap_or_default(),
    })
}

fn has_capability(organization: &Value, name: &str) -> bool {
    organization
        .get("capabilities")
        .and_then(Value::as_array)
        .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(name)))
}

/// The credential secret: the cookie plus the facts every call needs.
pub(super) fn secret(
    cookie: &str,
    device_id: Option<&str>,
    bootstrap: &Bootstrap,
    validated_at_ms: i64,
) -> Value {
    let mut secret = Map::new();
    secret.insert("cookie".into(), Value::String(cookie.to_owned()));
    secret.insert(
        "organization_uuid".into(),
        Value::String(bootstrap.organization.clone()),
    );
    if let Some(device) = device_id {
        secret.insert("device_id".into(), Value::String(device.to_owned()));
    }
    secret.insert("capabilities".into(), bootstrap.capabilities.clone());
    secret.insert("validated_at_ms".into(), Value::from(validated_at_ms));
    Value::Object(secret)
}

/// Public facts the host persists on the credential row.
pub(super) fn metadata(bootstrap: &Bootstrap, validated_at_ms: i64) -> Value {
    let mut metadata = Map::new();
    metadata.insert(
        "organization_uuid".into(),
        Value::String(bootstrap.organization.clone()),
    );
    metadata.insert("capabilities".into(), bootstrap.capabilities.clone());
    metadata.insert(
        "pro".into(),
        Value::Bool(super::auth::is_paid(Some(&bootstrap.capabilities))),
    );
    if let Some(email) = &bootstrap.email {
        metadata.insert("user_email".into(), Value::String(email.clone()));
    }
    if let Some(tier) = &bootstrap.rate_limit_tier {
        metadata.insert("rate_limit_tier".into(), tier.clone());
    }
    if let Some(flags) = &bootstrap.active_flags {
        metadata.insert("active_flags".into(), flags.clone());
    }
    metadata.insert("models".into(), models::to_metadata(&bootstrap.models));
    metadata.insert("validated_at_ms".into(), Value::from(validated_at_ms));
    Value::Object(metadata)
}
