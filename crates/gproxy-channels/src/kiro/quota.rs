//! Kiro quota — the captured Kiro CLI `GetUsageLimits` Smithy call:
//! `POST https://management.{region}.kiro.dev/?profileArn=…&origin=KIRO_CLI&isEmailRequired=true`
//! (the API requires the profileArn in both query and body). The breakdown
//! reports fractional credit units per resource type; `nextDateReset` is a
//! unix timestamp in seconds or milliseconds depending on server generation.

use bytes::Bytes;
use gproxy_channel_api::{ChannelError, QuotaObservation};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Value, json};

const TARGET_USAGE: &str = "AmazonCodeWhispererService.GetUsageLimits";

pub(super) fn probe_request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<http::Request<Bytes>>, ChannelError> {
    let token = super::auth::access_token(secret)?;
    let Some(profile) = super::auth::profile_arn(secret, settings) else {
        return Ok(None);
    };
    let query = format!(
        "profileArn={}&origin=KIRO_CLI&isEmailRequired=true",
        crate::shared::http::encode_component(profile)
    );
    let uri =
        crate::shared::http::join(&super::endpoint::management(settings)?, "/", Some(&query))?;
    let body = Bytes::from(
        serde_json::to_vec(&json!({
            "profileArn": profile,
            "origin": "KIRO_CLI",
            "isEmailRequired": true,
        }))
        .map_err(|error| ChannelError::Prepare(format!("Kiro usage body: {error}")))?,
    );
    let mut prepared = super::prepare::prepared(
        http::HeaderMap::new(),
        uri,
        body,
        token,
        TARGET_USAGE,
        super::prepare::UA_MANAGEMENT,
        false,
    )?;
    prepared.apply_profile();
    Ok(Some(prepared.request))
}

pub(super) fn parse_probe(status: http::StatusCode, body: &[u8]) -> Vec<QuotaObservation> {
    if !status.is_success() {
        return Vec::new();
    }
    let Ok(payload) = serde_json::from_slice::<UsageLimits>(body) else {
        return Vec::new();
    };
    let single = payload.usage_breakdown_list.len() == 1;
    payload
        .usage_breakdown_list
        .iter()
        .enumerate()
        .map(|(index, item)| QuotaObservation {
            unit: None,
            reset_behavior: gproxy_channel_api::QuotaResetBehavior::Periodic,
            scope: gproxy_channel_api::QuotaScope::All,
            sample: None,
            window_key: item
                .resource_type
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| crate::shared::quota::slug(value, "usage"))
                .unwrap_or_else(|| {
                    if single {
                        "agentic_request".to_owned()
                    } else {
                        format!("usage_{index}")
                    }
                }),
            label: None,
            period_start: None,
            period_end: item
                .next_date_reset
                .as_ref()
                .and_then(epoch_seconds)
                .or_else(|| payload.next_date_reset.as_ref().and_then(epoch_seconds)),
            used_percent: None,
            upstream_used: item
                .current_usage_with_precision
                .and_then(|value| Decimal::try_from(value).ok()),
            upstream_limit: item
                .usage_limit_with_precision
                .and_then(|value| Decimal::try_from(value).ok()),
        })
        .collect()
}

/// Milliseconds vs seconds split at 1e12 (~year 33658 in seconds).
fn epoch_seconds(value: &Value) -> Option<i64> {
    let value = match value {
        Value::Number(value) => value.as_f64()?,
        Value::String(value) => value.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    if !value.is_finite() {
        return None;
    }
    let seconds = if value.abs() >= 1_000_000_000_000.0 {
        value / 1000.0
    } else {
        value
    };
    Some(seconds as i64)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageLimits {
    next_date_reset: Option<Value>,
    #[serde(default)]
    usage_breakdown_list: Vec<UsageBreakdown>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageBreakdown {
    resource_type: Option<String>,
    current_usage_with_precision: Option<f64>,
    usage_limit_with_precision: Option<f64>,
    next_date_reset: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::parse_probe;

    #[test]
    fn breakdown_becomes_observations_with_millisecond_reset_normalized() {
        let body = br#"{
          "nextDateReset": 1735689600000,
          "subscriptionInfo": {"subscriptionTitle": "KIRO PRO+"},
          "usageBreakdownList": [
            {"currentUsage": 120, "currentUsageWithPrecision": 120.5,
             "usageLimit": 1000, "usageLimitWithPrecision": 1000.0}
          ]
        }"#;
        let observed = parse_probe(http::StatusCode::OK, body);
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].window_key, "agentic_request");
        assert_eq!(observed[0].upstream_used, Some("120.5".parse().unwrap()));
        assert_eq!(observed[0].upstream_limit, Some("1000".parse().unwrap()));
        assert_eq!(observed[0].period_end, Some(1_735_689_600));
    }

    #[test]
    fn resource_types_key_multiple_windows() {
        let body = br#"{"usageBreakdownList":[
          {"resourceType":"AGENTIC_REQUEST","currentUsageWithPrecision":1.0,
           "usageLimitWithPrecision":100.0,"nextDateReset":1735689600},
          {"resourceType":"Spec Task","currentUsageWithPrecision":2.0,
           "usageLimitWithPrecision":50.0}
        ]}"#;
        let observed = parse_probe(http::StatusCode::OK, body);
        assert_eq!(observed[0].window_key, "agentic_request");
        assert_eq!(observed[0].period_end, Some(1_735_689_600));
        assert_eq!(observed[1].window_key, "spec_task");
        assert_eq!(observed[1].period_end, None);
    }
}
