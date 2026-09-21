//! Two products, two quota surfaces.
//!
//! A Kimi Code subscription answers `GET {base}/usages`: a top-level `usage`
//! object is the rolling weekly allowance, and each `limits[]` entry adds a
//! `detail` of `used`/`limit` (numbers or numeric strings), an ISO-8601
//! `resetTime` and a declared `window` of `{duration, timeUnit}`. A Moonshot
//! platform key answers `GET /v1/users/me/balance` with a cash balance
//! instead. Which one is asked follows the credential, because that is what
//! decides which upstream the request can even authenticate against.

use super::auth::{self, Mode};
use super::config::{ID, KimiConfig};
use super::oauth::unix_now_ms;
use crate::channel::{
    CredentialContext, CredentialView, OperationFuture, ProviderView, QuotaAllowance, QuotaBalance,
    QuotaDimension, QuotaEntry, QuotaMetric, QuotaModel, QuotaQuery, QuotaResetBehavior,
    QuotaScope, QuotaSnapshot, QuotaSubject, QuotaTracking, QuotaValue, QuotaWindow,
};
use crate::channels::shared::compatible::ability::{bearer, decimal, require_success, send};
use crate::channels::shared::compatible::http::invalid_response;
use http::Method;
use rust_decimal::Decimal;
use serde_json::Value;

/// The subscription's rolling weekly allowance.
pub const WEEKLY_DIMENSION: &str = "weekly_limit";
/// The platform account's cash balance.
pub const BALANCE_DIMENSION: &str = "kimi_balance";

const WEEK_SECONDS: i64 = 7 * 24 * 60 * 60;

impl QuotaModel for super::Kimi {
    fn dimensions(
        &self,
        _: ProviderView<'_>,
        credential: CredentialView<'_>,
    ) -> Vec<QuotaDimension> {
        match auth::mode(&credential) {
            // Only the weekly window is declared. The shorter windows the
            // upstream reports are named by the plan, not by the wire, so they
            // arrive as observations under ids read from the reply.
            Mode::Subscription => vec![QuotaDimension {
                id: WEEKLY_DIMENSION.into(),
                label: Some("weekly subscription allowance".into()),
                scope: QuotaScope::All,
                operations: None,
                metric: QuotaMetric::Unit("kimi_credit".into()),
                window: QuotaWindow::Rolling {
                    seconds: WEEK_SECONDS,
                },
                limit: None,
                tracking: QuotaTracking::Reported,
            }],
            Mode::ApiKey => vec![QuotaDimension {
                id: BALANCE_DIMENSION.into(),
                label: Some("account balance".into()),
                scope: QuotaScope::All,
                operations: None,
                metric: QuotaMetric::Cost,
                window: QuotaWindow::Total,
                limit: None,
                tracking: QuotaTracking::Reported,
            }],
        }
    }
}

fn text(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn iso_to_ms(value: &str) -> Option<i64> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|stamp| stamp.unix_timestamp() * 1_000)
}

/// `{duration, timeUnit}` as seconds; an unfamiliar unit is no window rather
/// than a wrong one.
fn window_seconds(value: &Value) -> Option<i64> {
    let duration = match value.get("duration")? {
        Value::Number(number) => number.as_i64()?,
        Value::String(text) => text.trim().parse().ok()?,
        _ => return None,
    };
    let multiplier = match text(value.get("timeUnit"))? {
        "TIME_UNIT_MINUTE" => 60,
        "TIME_UNIT_HOUR" => 60 * 60,
        "TIME_UNIT_DAY" => 24 * 60 * 60,
        "TIME_UNIT_WEEK" => WEEK_SECONDS,
        _ => return None,
    };
    duration.checked_mul(multiplier)
}

/// A lowercase, underscore-joined key from a plan's label for a window.
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

/// One `{used, limit, resetTime}` record. `used` may be implied by
/// `remaining`; a record that states neither a use nor a limit is no reading.
fn allowance(record: &Value, seconds: Option<i64>) -> Option<QuotaAllowance> {
    let limit = record.get("limit").and_then(decimal);
    let remaining = record.get("remaining").and_then(decimal);
    let used = record.get("used").and_then(decimal).or_else(|| {
        limit
            .zip(remaining)
            .map(|(limit, remaining)| limit - remaining)
    });
    if used.is_none() && limit.is_none() {
        return None;
    }
    let period_end_ms = text(record.get("resetTime")).and_then(iso_to_ms);
    Some(QuotaAllowance {
        used,
        limit,
        remaining: remaining.or_else(|| limit.zip(used).map(|(limit, used)| limit - used)),
        used_percent: used.zip(limit).and_then(|(used, limit)| {
            (!limit.is_zero()).then(|| used / limit * Decimal::from(100))
        }),
        unlimited: None,
        unit: None,
        // The reply dates the reset, not the start; the declared window length
        // is what makes a start knowable at all.
        period_start_ms: period_end_ms
            .zip(seconds.filter(|seconds| *seconds > 0))
            .map(|(end, seconds)| end - seconds * 1_000),
        period_end_ms,
        reset_behavior: QuotaResetBehavior::Periodic,
    })
}

fn subscription_entries(payload: &Value) -> Vec<QuotaEntry> {
    let mut entries = Vec::new();
    if let Some(allowance) = payload
        .get("usage")
        .and_then(|usage| allowance(usage, Some(WEEK_SECONDS)))
    {
        entries.push(QuotaEntry {
            id: WEEKLY_DIMENSION.into(),
            source_id: WEEKLY_DIMENSION.into(),
            label: Some("weekly".into()),
            subject: QuotaSubject::Account,
            model_scope: QuotaScope::All,
            value: QuotaValue::Window(allowance),
        });
    }
    for (index, record) in payload
        .get("limits")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        let Some(detail) = record.get("detail").filter(|value| value.is_object()) else {
            continue;
        };
        let label = text(record.get("name")).or_else(|| text(detail.get("name")));
        let id = label
            .map(slug)
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| format!("limit_{index}"));
        let seconds = record.get("window").and_then(window_seconds);
        let Some(allowance) = allowance(detail, seconds) else {
            continue;
        };
        entries.push(QuotaEntry {
            id: id.clone(),
            source_id: id,
            label: label.map(str::to_owned),
            subject: QuotaSubject::Account,
            model_scope: QuotaScope::All,
            value: QuotaValue::Window(allowance),
        });
    }
    entries
}

/// `{"data": {"available_balance": …, "voucher_balance": …, "cash_balance": …}}`.
fn balance_entries(payload: &Value) -> Vec<QuotaEntry> {
    let record = payload.get("data").unwrap_or(payload);
    let remaining = ["available_balance", "cash_balance", "balance"]
        .iter()
        .find_map(|name| record.get(*name).and_then(decimal));
    if remaining.is_none() {
        return Vec::new();
    }
    vec![QuotaEntry {
        id: BALANCE_DIMENSION.into(),
        source_id: BALANCE_DIMENSION.into(),
        label: Some("balance".into()),
        subject: QuotaSubject::Account,
        model_scope: QuotaScope::All,
        value: QuotaValue::Balance(QuotaBalance {
            remaining,
            unit: Some("CNY".into()),
        }),
    }]
}

impl QuotaQuery for super::Kimi {
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let config = KimiConfig::from_view(context.provider)?;
            let mode = auth::mode(&context.credential);
            let token = auth::token(&context.credential, mode)?;
            let base = auth::base_url(context.provider, &context.credential, mode);
            let mut headers = bearer(token)?;
            let path = match mode {
                Mode::Subscription => {
                    auth::identity(&mut headers, &config, &context.credential)?;
                    "/usages"
                }
                Mode::ApiKey => "/v1/users/me/balance",
            };
            let url = format!("{base}{}", auth::path(mode, path));
            let (status, _, body) = send(context.client, Method::GET, &url, headers, None).await?;
            let body = require_success(status, body)?;
            let payload: Value = serde_json::from_slice(&body)
                .map_err(|error| invalid_response(format!("{ID} quota: {error}")))?;
            let entries = match mode {
                Mode::Subscription => subscription_entries(&payload),
                Mode::ApiKey => balance_entries(&payload),
            };
            if entries.is_empty() {
                return Err(invalid_response(format!("{ID} quota: no readings")));
            }
            Ok(QuotaSnapshot {
                observed_at_ms: unix_now_ms(),
                entries,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_weekly_summary_and_each_named_limit_become_readings() {
        let payload = json!({
            "usage": {"used": "40", "limit": "1000", "resetTime": "2030-01-08T00:00:00Z"},
            "limits": [{
                "name": "Five hour",
                "window": {"duration": 300, "timeUnit": "TIME_UNIT_MINUTE"},
                "detail": {"used": "5", "limit": "100", "resetTime": "2030-01-01T05:00:00Z"}
            }],
        });
        let entries = subscription_entries(&payload);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, WEEKLY_DIMENSION);
        let QuotaValue::Window(weekly) = &entries[0].value else {
            panic!("a window");
        };
        assert_eq!(weekly.used_percent, Some(Decimal::from(4)));
        assert_eq!(weekly.remaining, Some(Decimal::from(960)));
        assert_eq!(
            weekly.period_start_ms,
            Some(weekly.period_end_ms.unwrap() - WEEK_SECONDS * 1_000)
        );
        assert_eq!(entries[1].id, "five_hour");
        let QuotaValue::Window(hourly) = &entries[1].value else {
            panic!("a window");
        };
        assert_eq!(
            hourly.period_start_ms,
            Some(hourly.period_end_ms.unwrap() - 5 * 60 * 60 * 1_000)
        );
    }

    #[test]
    fn a_use_may_be_implied_by_what_remains_and_a_bare_limit_is_no_reading() {
        let payload = json!({
            "usage": {"limit": 100, "remaining": 25},
            "limits": [{"detail": {"name": "empty"}}],
        });
        let entries = subscription_entries(&payload);
        assert_eq!(entries.len(), 1, "a detail with neither count is skipped");
        let QuotaValue::Window(weekly) = &entries[0].value else {
            panic!("a window");
        };
        assert_eq!(weekly.used, Some(Decimal::from(75)));
        assert_eq!(weekly.used_percent, Some(Decimal::from(75)));
        assert_eq!(weekly.period_end_ms, None);
    }

    #[test]
    fn a_platform_balance_reads_through_its_envelope() {
        let entries = balance_entries(&json!({"data": {"available_balance": 12.5}}));
        assert_eq!(
            entries[0].value,
            QuotaValue::Balance(QuotaBalance {
                remaining: Some("12.5".parse().unwrap()),
                unit: Some("CNY".into()),
            })
        );
        assert!(balance_entries(&json!({"data": {}})).is_empty());
    }
}
