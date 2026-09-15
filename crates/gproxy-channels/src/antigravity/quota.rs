//! Prefer the account summary: Gemini and third-party models each have distinct
//! five-hour and weekly buckets. Older accounts may use per-model quotaInfo.

use bytes::Bytes;
use gproxy_channel_api::{ChannelError, QuotaObservation, QuotaResetBehavior, QuotaScope};
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use serde_json::Value;

pub(super) fn probe_request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<http::Request<Bytes>>, ChannelError> {
    // The summary is a production account API, not an inference operation.
    // Explicit provider base/endpoint overrides still take precedence.
    let mut settings = settings.clone();
    if settings
        .get("base_url")
        .and_then(Value::as_str)
        .is_none_or(|s| s.trim().is_empty())
    {
        settings["base_url"] = Value::String("https://cloudcode-pa.googleapis.com".into());
    }
    request(
        secret,
        &settings,
        "quota_summary",
        "/v1internal:retrieveUserQuotaSummary",
    )
}

pub(super) fn legacy_probe_request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<http::Request<Bytes>>, ChannelError> {
    request(
        secret,
        settings,
        "gemini_list_models",
        "/v1internal:fetchAvailableModels",
    )
}

fn request(
    secret: &Value,
    settings: &Value,
    operation: &str,
    path: &str,
) -> Result<Option<http::Request<Bytes>>, ChannelError> {
    let access = super::auth::access_token(secret)?;
    let uri = super::prepare::endpoint_uri(settings, operation, path, None)?;
    let mut body = serde_json::json!({});
    if let Some(project) = secret
        .get("project_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        body["project"] = Value::String(project.to_owned());
    }
    http::Request::post(uri)
        .header(
            AUTHORIZATION,
            http::HeaderValue::from_str(&format!("Bearer {access}"))
                .map_err(|error| ChannelError::Secret(error.to_string()))?,
        )
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, super::prepare::USER_AGENT_VALUE)
        .body(Bytes::from(body.to_string()))
        .map(Some)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

pub(super) fn parse_probe(status: http::StatusCode, body: &[u8]) -> Vec<QuotaObservation> {
    if !status.is_success() {
        return Vec::new();
    }
    let Ok(raw) = serde_json::from_slice::<Value>(body) else {
        return Vec::new();
    };
    let root = raw
        .get("quotaSummary")
        .or_else(|| raw.get("quota_summary"))
        .or_else(|| raw.get("userQuotaSummary"))
        .or_else(|| raw.get("user_quota_summary"))
        .unwrap_or(&raw);
    if root.get("groups").is_some() || root.get("buckets").is_some() {
        return parse_summary(root);
    }
    let Some(models) = root.get("models").and_then(Value::as_object) else {
        return Vec::new();
    };
    models
        .iter()
        .filter_map(|(model_id, model)| {
            let quota = model.get("quotaInfo").filter(|value| value.is_object())?;
            Some(QuotaObservation {
                unit: None,
                reset_behavior: gproxy_channel_api::QuotaResetBehavior::Periodic,
                scope: gproxy_channel_api::QuotaScope::Models(vec![model_id.clone()]),
                sample: None,
                window_key: model_id.clone(),
                label: None,
                period_start: None,
                period_end: quota
                    .get("resetTime")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|reset| !reset.is_empty())
                    .and_then(crate::shared::quota::iso_to_unix),
                used_percent: quota
                    .get("remainingFraction")
                    .and_then(Value::as_f64)
                    .and_then(crate::shared::quota::remaining_fraction_to_used_percent),
                upstream_used: None,
                upstream_limit: None,
            })
        })
        .collect()
}

fn parse_summary(raw: &Value) -> Vec<QuotaObservation> {
    let mut observations = std::collections::BTreeMap::new();
    let mut add = |bucket: &Value, group: Option<&str>| {
        if let Some(observation) = summary_bucket(bucket, group) {
            observations.insert(observation.window_key.clone(), observation);
        }
    };
    if let Some(groups) = raw.get("groups").and_then(Value::as_array) {
        for group in groups {
            let name = text(group, "displayName", "display_name");
            if let Some(buckets) = group.get("buckets").and_then(Value::as_array) {
                for bucket in buckets {
                    add(bucket, name);
                }
            }
        }
    }
    if let Some(buckets) = raw.get("buckets").and_then(Value::as_array) {
        for bucket in buckets {
            add(bucket, None);
        }
    }
    observations.into_values().collect()
}

fn text<'a>(value: &'a Value, camel: &str, snake: &str) -> Option<&'a str> {
    value
        .get(camel)
        .or_else(|| value.get(snake))
        .and_then(Value::as_str)
}

fn summary_bucket(bucket: &Value, group: Option<&str>) -> Option<QuotaObservation> {
    let id = text(bucket, "bucketId", "bucket_id")
        .unwrap_or("")
        .to_ascii_lowercase();
    let group = group.unwrap_or("").to_ascii_lowercase();
    let (family, prefixes) =
        if id.starts_with("gemini-") || (id.is_empty() && group.contains("gemini")) {
            ("gemini", vec!["gemini".into()])
        } else if id.starts_with("3p-")
            || (id.is_empty() && (group.contains("claude") || group.contains("gpt")))
        {
            ("3p", vec!["claude".into(), "gpt".into()])
        } else {
            return None;
        };
    let period = bucket
        .get("window")
        .and_then(Value::as_str)
        .or_else(|| id.rsplit_once('-').map(|(_, period)| period))?;
    let normalized: String = period
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    let period = match normalized.as_str() {
        "5h" | "5hour" | "5hours" | "fivehour" | "fivehours" | "300m" => "5h",
        "weekly" | "week" | "7d" | "7day" | "7days" | "sevenday" | "sevendays" | "168h" => "weekly",
        _ => return None,
    };
    let fraction = bucket
        .get("remainingFraction")
        .or_else(|| bucket.get("remaining_fraction"))
        .and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
        })
        .filter(|value| value.is_finite() && (0.0..=1.0).contains(value));
    // When the weekly cap is exhausted Google disables the 5h bucket and
    // returns remainingFraction=1. That is not usable quota. Preserve a
    // display marker without changing the stable window key or DTO schema.
    let disabled = bucket.get("disabled").and_then(Value::as_bool) == Some(true);
    Some(QuotaObservation {
        unit: None,
        reset_behavior: QuotaResetBehavior::Periodic,
        scope: QuotaScope::ModelPrefixes(prefixes),
        sample: None,
        window_key: format!("{family}-{period}"),
        label: disabled.then(|| "antigravity_disabled".into()),
        period_start: None,
        period_end: text(bucket, "resetTime", "reset_time")
            .and_then(crate::shared::quota::iso_to_unix),
        used_percent: fraction
            .filter(|_| !disabled)
            .and_then(crate::shared::quota::remaining_fraction_to_used_percent),
        upstream_used: None,
        upstream_limit: None,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_probe;

    #[test]
    fn models_with_quota_info_become_observations() {
        let body = br#"{"models":{
          "gemini-3.1-pro":{"quotaInfo":{"remainingFraction":0.8,"resetTime":"2026-08-01T12:00:00Z"}},
          "no-quota":{},
          "null-quota":{"quotaInfo":null}
        }}"#;
        let observed = parse_probe(http::StatusCode::OK, body);
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].window_key, "gemini-3.1-pro");
        assert_eq!(observed[0].used_percent, Some("20".parse().unwrap()));
        assert_eq!(observed[0].period_end, Some(1_785_585_600));
        assert_eq!(observed[0].period_start, None);
    }

    #[test]
    fn disabled_five_hour_bucket_is_not_reported_as_available_quota() {
        let body = br#"{"groups":[{"displayName":"Claude and GPT models","buckets":[
          {"bucketId":"3p-weekly","window":"weekly","remainingFraction":0},
          {"bucketId":"3p-5h","window":"5h","remainingFraction":1,"disabled":true}
        ]}]}"#;
        let observed = parse_probe(http::StatusCode::OK, body);
        let weekly = observed
            .iter()
            .find(|o| o.window_key == "3p-weekly")
            .unwrap();
        assert_eq!(weekly.used_percent, Some(100.into()));
        let five_hour = observed.iter().find(|o| o.window_key == "3p-5h").unwrap();
        assert_eq!(five_hour.used_percent, None);
        assert_eq!(five_hour.label.as_deref(), Some("antigravity_disabled"));
    }

    #[test]
    fn summary_keeps_four_independent_windows_and_model_scopes() {
        let body = br#"{"groups":[{"displayName":"Gemini Models","buckets":[
          {"bucketId":"gemini-5h","remainingFraction":0.75,"resetTime":"2026-09-15T12:00:00Z"},
          {"bucketId":"gemini-weekly","remainingFraction":0.5,"resetTime":"2026-09-20T12:00:00Z"}
        ]},{"displayName":"Claude and GPT models","buckets":[
          {"bucketId":"3p-5h","remainingFraction":0},
          {"bucketId":"3p-weekly","remainingFraction":1}
        ]}]}"#;
        let observed = parse_probe(http::StatusCode::OK, body);
        assert_eq!(observed.len(), 4);
        let gemini = observed
            .iter()
            .find(|o| o.window_key == "gemini-5h")
            .unwrap();
        assert_eq!(gemini.used_percent, Some(25.into()));
        assert!(gemini.scope.includes("gemini-pro-agent"));
        assert!(!gemini.scope.includes("claude-opus-4-6-thinking"));
        let third_party = observed.iter().find(|o| o.window_key == "3p-5h").unwrap();
        assert_eq!(third_party.used_percent, Some(100.into()));
        assert!(third_party.scope.includes("claude-opus-4-6-thinking"));
        assert!(third_party.scope.includes("gpt-oss-120b-medium"));
        assert_eq!(gemini.period_start, None);
        assert_ne!(
            gemini.period_end,
            observed
                .iter()
                .find(|o| o.window_key == "gemini-weekly")
                .unwrap()
                .period_end
        );
    }

    #[test]
    fn missing_or_invalid_fraction_is_unknown_not_full() {
        let body = br#"{"quota_summary":{"buckets":[
          {"bucket_id":"gemini-5h","window":"FIVE_HOURS","reset_time":"2026-09-15T12:00:00Z"},
          {"bucket_id":"gemini-weekly","remaining_fraction":2},
          {"bucket_id":"3p-5h","remaining_fraction":"0.25"},
          {"bucket_id":"unknown-5h","remaining_fraction":1}
        ]}}"#;
        let observed = parse_probe(http::StatusCode::OK, body);
        assert_eq!(observed.len(), 3);
        assert!(
            observed
                .iter()
                .filter(|o| o.window_key.starts_with("gemini-"))
                .all(|o| o.used_percent.is_none())
        );
        assert_eq!(
            observed
                .iter()
                .find(|o| o.window_key == "3p-5h")
                .unwrap()
                .used_percent,
            Some(75.into())
        );
        assert!(parse_probe(http::StatusCode::UNAUTHORIZED, body).is_empty());
    }

    #[test]
    fn summary_and_fallback_send_the_project_without_changing_inference_endpoints() {
        let secret = serde_json::json!({"access_token":"fixture", "project_id":"test-project"});
        let request = super::probe_request(&secret, &serde_json::json!({}))
            .unwrap()
            .unwrap();
        assert_eq!(
            request.uri(),
            "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary"
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(request.body()).unwrap()["project"],
            "test-project"
        );
        let fallback = super::legacy_probe_request(&secret, &serde_json::json!({}))
            .unwrap()
            .unwrap();
        assert_eq!(
            fallback.uri(),
            "https://daily-cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels"
        );
    }
}
