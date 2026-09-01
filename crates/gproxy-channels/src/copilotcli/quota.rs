//! Copilot quota — `GET api.github.com/copilot_internal/user`. The
//! `quota_snapshots` object carries one entry per metered feature (`chat`,
//! `completions`, `premium_interactions`) with an entitlement, a remaining
//! count, and a `percent_remaining`; `unlimited: true` means the feature is
//! not metered at all. `quota_reset_date` is a bare `YYYY-MM-DD` date.

use bytes::Bytes;
use gproxy_channel_api::{ChannelError, QuotaObservation};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;

const USAGE_URL: &str = "https://api.github.com/copilot_internal/user";

pub(super) fn probe_request(
    secret: &Value,
    _settings: &Value,
) -> Result<Option<http::Request<Bytes>>, ChannelError> {
    super::auth::github_request(secret, USAGE_URL).map(Some)
}

pub(super) fn parse_probe(status: http::StatusCode, body: &[u8]) -> Vec<QuotaObservation> {
    if !status.is_success() {
        return Vec::new();
    }
    let Ok(payload) = serde_json::from_slice::<CopilotUser>(body) else {
        return Vec::new();
    };
    let period_end = payload.quota_reset_date.as_deref().and_then(reset_to_unix);
    let snapshots = payload.quota_snapshots.unwrap_or_default();
    [
        ("chat", snapshots.chat),
        ("completions", snapshots.completions),
        ("premium_interactions", snapshots.premium_interactions),
    ]
    .into_iter()
    .filter_map(|(window_key, detail)| {
        let detail = detail?;
        if detail.unlimited == Some(true) {
            return None;
        }
        Some(QuotaObservation {
            window_key: window_key.to_owned(),
            label: None,
            period_start: None,
            period_end,
            used_percent: detail
                .percent_remaining
                .and_then(|percent| Decimal::try_from((100.0 - percent).clamp(0.0, 100.0)).ok()),
            upstream_used: detail.entitlement.zip(detail.remaining).and_then(
                |(entitlement, remaining)| {
                    Decimal::try_from((entitlement - remaining).max(0.0)).ok()
                },
            ),
            upstream_limit: detail
                .entitlement
                .and_then(|value| Decimal::try_from(value).ok()),
        })
    })
    .collect()
}

fn reset_to_unix(value: &str) -> Option<i64> {
    if let Some(unix) = crate::shared::quota::iso_to_unix(value) {
        return Some(unix);
    }
    let mut parts = value.trim().split('-');
    let year = parts.next()?.parse().ok()?;
    let month: u8 = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    time::Date::from_calendar_date(year, time::Month::try_from(month).ok()?, day)
        .ok()
        .map(|date| date.midnight().assume_utc().unix_timestamp())
}

#[derive(Deserialize)]
struct CopilotUser {
    quota_reset_date: Option<String>,
    quota_snapshots: Option<QuotaSnapshots>,
}

#[derive(Deserialize, Default)]
struct QuotaSnapshots {
    chat: Option<QuotaDetail>,
    completions: Option<QuotaDetail>,
    premium_interactions: Option<QuotaDetail>,
}

#[derive(Deserialize)]
struct QuotaDetail {
    entitlement: Option<f64>,
    remaining: Option<f64>,
    percent_remaining: Option<f64>,
    unlimited: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::parse_probe;

    #[test]
    fn metered_snapshots_become_observations_and_unlimited_are_skipped() {
        let body = br#"{
          "copilot_plan": "pro",
          "quota_reset_date": "2026-07-01",
          "quota_snapshots": {
            "chat": {"entitlement": 0, "remaining": 0, "percent_remaining": 100, "unlimited": true},
            "completions": {"entitlement": 0, "remaining": 0, "percent_remaining": 100, "unlimited": true},
            "premium_interactions": {"entitlement": 300, "remaining": 270, "percent_remaining": 90,
                                     "unlimited": false, "overage_count": 0}
          }
        }"#;
        let observed = parse_probe(http::StatusCode::OK, body);
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].window_key, "premium_interactions");
        assert_eq!(observed[0].used_percent, Some("10".parse().unwrap()));
        assert_eq!(observed[0].upstream_used, Some("30".parse().unwrap()));
        assert_eq!(observed[0].upstream_limit, Some("300".parse().unwrap()));
        // Bare-date reset resolves to midnight UTC.
        assert_eq!(observed[0].period_end, Some(1_782_864_000));
    }
}
