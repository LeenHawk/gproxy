//! Claude Code OAuth profile enrichment.
//!
//! `/api/oauth/profile` is supplemental account metadata: authentication stays
//! usable when this best-effort request fails. Keep product-only onboarding
//! fields out of the credential and retain only metadata useful to routing and
//! administration.

use std::sync::Arc;

use bytes::Bytes;
use serde_json::{Map, Value};

use crate::http::client::UpstreamClient;

pub(super) async fn enrich(client: &Arc<dyn UpstreamClient>, secret: &mut Value) {
    let Some(access_token) = secret
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
    else {
        return;
    };
    let Ok(mut request) = http::Request::builder()
        .method(http::Method::GET)
        .uri(format!(
            "{}/api/oauth/profile",
            super::auth::DEFAULT_BASE_URL
        ))
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {access_token}"),
        )
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Bytes::new())
    else {
        return;
    };
    super::axios::apply(&mut request, 10, true);
    let Ok(response) = client.send(request).await else {
        return;
    };
    let (parts, body) = response.into_parts();
    if !parts.status.is_success() {
        return;
    }
    let Ok(profile) = serde_json::from_slice::<Value>(&body) else {
        return;
    };
    merge(secret, &profile);
}

fn merge(secret: &mut Value, profile: &Value) {
    let Some(secret) = secret.as_object_mut() else {
        return;
    };
    let account = profile.get("account");
    insert_string(
        secret,
        "user_email",
        account.and_then(|value| value.get("email")),
    );
    insert_string(
        secret,
        "account_uuid",
        account.and_then(|value| value.get("uuid")),
    );

    let organization = profile.get("organization");
    insert_string(
        secret,
        "organization_uuid",
        organization.and_then(|value| value.get("uuid")),
    );
    insert_string(
        secret,
        "organization_type",
        organization.and_then(|value| value.get("organization_type")),
    );
    insert_string(
        secret,
        "rate_limit_tier",
        organization.and_then(|value| value.get("rate_limit_tier")),
    );
    insert_string(
        secret,
        "seat_tier",
        organization.and_then(|value| value.get("seat_tier")),
    );
    insert_string(
        secret,
        "billing_type",
        organization.and_then(|value| value.get("billing_type")),
    );
    if let Some(enabled) = organization
        .and_then(|value| value.get("has_extra_usage_enabled"))
        .and_then(Value::as_bool)
    {
        secret.insert("has_extra_usage_enabled".into(), Value::Bool(enabled));
    }
}

fn insert_string(secret: &mut Map<String, Value>, key: &str, value: Option<&Value>) {
    let Some(value) = value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    secret.insert(key.into(), Value::String(value.to_owned()));
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn merges_proxy_relevant_profile_fields() {
        let mut secret = json!({"access_token": "token"});
        merge(
            &mut secret,
            &json!({
                "account": {"uuid": "acct-1", "email": "user@example.com"},
                "organization": {
                    "uuid": "org-1",
                    "organization_type": "claude_max",
                    "rate_limit_tier": "default_claude_max_20x",
                    "seat_tier": "max_20x",
                    "has_extra_usage_enabled": true,
                    "billing_type": "stripe_subscription"
                }
            }),
        );

        assert_eq!(secret["account_uuid"], "acct-1");
        assert_eq!(secret["user_email"], "user@example.com");
        assert_eq!(secret["organization_uuid"], "org-1");
        assert_eq!(secret["organization_type"], "claude_max");
        assert_eq!(secret["rate_limit_tier"], "default_claude_max_20x");
        assert_eq!(secret["seat_tier"], "max_20x");
        assert_eq!(secret["has_extra_usage_enabled"], true);
        assert_eq!(secret["billing_type"], "stripe_subscription");
    }

    #[test]
    fn malformed_or_missing_fields_preserve_existing_metadata() {
        let mut secret = json!({
            "user_email": "old@example.com",
            "organization_type": "claude_pro",
            "has_extra_usage_enabled": true,
            "billing_type": "existing"
        });
        merge(
            &mut secret,
            &json!({
                "account": {"email": "  "},
                "organization": {
                    "organization_type": 7,
                    "has_extra_usage_enabled": "false",
                    "billing_type": null
                }
            }),
        );

        assert_eq!(secret["user_email"], "old@example.com");
        assert_eq!(secret["organization_type"], "claude_pro");
        assert_eq!(secret["has_extra_usage_enabled"], true);
        assert_eq!(secret["billing_type"], "existing");
    }
}
