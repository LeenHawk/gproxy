//! What the subscription has left.
//!
//! `AmazonCodeWhispererService.GetUsageLimits` on the management plane, as the
//! Kiro CLI calls it (v3 `kiro/quota.rs`): `POST {management}/` with the
//! profile ARN in *both* the query and the body, which is what the API
//! requires. The reply breaks usage down per resource type with fractional
//! credit units, and dates each reset with a unix timestamp that is seconds
//! on some server generations and milliseconds on others.
//!
//! There is no `QuotaModel`: which buckets a credential has is only knowable
//! once the upstream has answered, so nothing on the credential could declare
//! them — the same reading `geminicli` and `antigravity` take.

use super::config::{ID, KiroConfig};
use super::{Plane, Target, encode_component, send, unix_now_ms};
use crate::channel::{
    ChannelError, CredentialContext, OperationFuture, QuotaAllowance, QuotaEntry, QuotaQuery,
    QuotaResetBehavior, QuotaScope, QuotaSnapshot, QuotaSubject, QuotaValue,
};
use http::Method;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Value, json};

/// The Smithy operation the CLI names in `x-amz-target`.
pub const TARGET_USAGE_LIMITS: &str = "AmazonCodeWhispererService.GetUsageLimits";
/// The one window a single-entry breakdown means (v3 `quota.rs`).
pub const AGENTIC_REQUEST_DIMENSION: &str = "agentic_request";

/// Milliseconds and seconds are told apart at 1e12, which is the year 33658
/// read as seconds.
const MILLISECOND_FLOOR: f64 = 1_000_000_000_000.0;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageLimits {
    #[serde(default)]
    next_date_reset: Option<Value>,
    #[serde(default)]
    usage_breakdown_list: Vec<UsageBreakdown>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageBreakdown {
    #[serde(default)]
    resource_type: Option<String>,
    #[serde(default)]
    current_usage_with_precision: Option<f64>,
    #[serde(default)]
    usage_limit_with_precision: Option<f64>,
    #[serde(default)]
    next_date_reset: Option<Value>,
}

/// A lowercase, underscore-joined key from a resource type.
fn slug(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    for character in label.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_owned()
}

fn epoch_ms(value: Option<&Value>) -> Option<i64> {
    let value = match value? {
        Value::Number(number) => number.as_f64()?,
        Value::String(text) => text.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    if !value.is_finite() {
        return None;
    }
    let milliseconds = if value.abs() >= MILLISECOND_FLOOR {
        value
    } else {
        value * 1_000.0
    };
    Some(milliseconds as i64)
}

fn decimal(value: Option<f64>) -> Option<Decimal> {
    value.and_then(|value| Decimal::try_from(value).ok())
}

pub(super) fn entries(body: &[u8]) -> Result<Vec<QuotaEntry>, ChannelError> {
    let payload: UsageLimits = serde_json::from_slice(body)
        .map_err(|error| ChannelError::InvalidResponse(format!("{ID} usage: {error}")))?;
    let single = payload.usage_breakdown_list.len() == 1;
    Ok(payload
        .usage_breakdown_list
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let id = item
                .resource_type
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(slug)
                .filter(|slug| !slug.is_empty())
                .unwrap_or_else(|| {
                    if single {
                        AGENTIC_REQUEST_DIMENSION.to_owned()
                    } else {
                        format!("usage_{index}")
                    }
                });
            let used = decimal(item.current_usage_with_precision);
            let limit = decimal(item.usage_limit_with_precision);
            QuotaEntry {
                id: id.clone(),
                source_id: id,
                label: item.resource_type.clone(),
                subject: QuotaSubject::Account,
                model_scope: QuotaScope::All,
                value: QuotaValue::Window(QuotaAllowance {
                    used,
                    limit,
                    remaining: limit.zip(used).map(|(limit, used)| limit.saturating_sub(used)),
                    used_percent: used.zip(limit).and_then(|(used, limit)| {
                        used
            .checked_div(limit)
            .and_then(|ratio| ratio.checked_mul(Decimal::ONE_HUNDRED))
                    }),
                    unlimited: None,
                    unit: None,
                    // The reply dates the reset and nothing else; a start is
                    // not inferred from it.
                    period_start_ms: None,
                    period_end_ms: epoch_ms(item.next_date_reset.as_ref())
                        .or_else(|| epoch_ms(payload.next_date_reset.as_ref())),
                    reset_behavior: QuotaResetBehavior::Periodic,
                }),
            }
        })
        .collect())
}

impl QuotaQuery for super::Kiro {
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let config = KiroConfig::from_view(context.provider)?;
            let profile = super::profile_arn(&config, &context.credential)
                .ok_or(ChannelError::InvalidCredential)?;
            let query = format!(
                "profileArn={}&origin=KIRO_CLI&isEmailRequired=true",
                encode_component(&profile)
            );
            let url = format!(
                "{}/?{query}",
                super::plane_base(context.provider, &config, Plane::Management)?
            );
            let body = json!({
                "profileArn": profile,
                "origin": "KIRO_CLI",
                "isEmailRequired": true,
            });
            let headers = super::smithy_headers(
                http::HeaderMap::new(),
                &config,
                super::access_token(&context.credential)?,
                Target::UsageLimits,
                &body.to_string().into_bytes(),
            )?;
            let (status, _, bytes) = send(
                context.client,
                Method::POST,
                &url,
                headers,
                Some(body.to_string().into_bytes()),
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                });
            }
            Ok(QuotaSnapshot {
                observed_at_ms: unix_now_ms(),
                entries: entries(&bytes)?,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_breakdown_is_the_agentic_request_window_and_a_millisecond_reset_stays_one() {
        let body = br#"{
          "nextDateReset": 1735689600000,
          "usageBreakdownList": [
            {"currentUsage": 120, "currentUsageWithPrecision": 120.5,
             "usageLimit": 1000, "usageLimitWithPrecision": 1000.0}
          ]
        }"#;
        let entries = entries(body).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, AGENTIC_REQUEST_DIMENSION);
        let QuotaValue::Window(window) = &entries[0].value else {
            panic!("a window");
        };
        assert_eq!(window.used, Some("120.5".parse().unwrap()));
        assert_eq!(window.limit, Some("1000".parse().unwrap()));
        assert_eq!(window.remaining, Some("879.5".parse().unwrap()));
        assert_eq!(window.period_end_ms, Some(1_735_689_600_000));
    }

    #[test]
    fn a_resource_type_names_its_own_window_and_seconds_become_milliseconds() {
        let body = br#"{"usageBreakdownList":[
          {"resourceType":"AGENTIC_REQUEST","currentUsageWithPrecision":1.0,
           "usageLimitWithPrecision":100.0,"nextDateReset":1735689600},
          {"resourceType":"Spec Task","currentUsageWithPrecision":2.0,
           "usageLimitWithPrecision":50.0}
        ]}"#;
        let entries = entries(body).unwrap();
        assert_eq!(entries[0].id, "agentic_request");
        assert_eq!(entries[0].label.as_deref(), Some("AGENTIC_REQUEST"));
        assert_eq!(entries[1].id, "spec_task");
        let QuotaValue::Window(first) = &entries[0].value else {
            panic!("a window");
        };
        assert_eq!(first.period_end_ms, Some(1_735_689_600_000));
        assert_eq!(first.used_percent, Some(Decimal::from(1)));
        let QuotaValue::Window(second) = &entries[1].value else {
            panic!("a window");
        };
        assert_eq!(
            second.period_end_ms, None,
            "no reset is not a reset at zero"
        );
    }
}
