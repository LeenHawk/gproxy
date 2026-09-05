//! Kimi Code subscription quota — `GET {base}/usages` with the OAuth bearer
//! (the coding-plan endpoint; plain API keys have no probe). The top-level
//! `usage` object is the rolling weekly allowance; each `limits[]` entry
//! carries a `detail` with `used`/`limit` (numbers or numeric strings), an
//! ISO-8601 `resetTime`, and a declared `window` `{duration, timeUnit}`.

use bytes::Bytes;
use gproxy_channel_api::{ChannelError, QuotaObservation};
use rust_decimal::Decimal;
use serde_json::Value;

const WEEK_SECONDS: i64 = 7 * 24 * 60 * 60;

pub(super) fn probe_request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<http::Request<Bytes>>, ChannelError> {
    if super::auth::mode(secret) != super::auth::Mode::Oauth {
        return Ok(None);
    }
    let uri = crate::shared::http::join(super::auth::base_url(settings, secret), "/usages", None)?;
    let mut headers = http::HeaderMap::new();
    super::auth::apply(&mut headers, secret, false, &http::Method::GET)?;
    let mut request = http::Request::get(uri)
        .body(Bytes::new())
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    *request.headers_mut() = headers;
    Ok(Some(request))
}

pub(super) fn parse_probe(status: http::StatusCode, body: &[u8]) -> Vec<QuotaObservation> {
    if !status.is_success() {
        return Vec::new();
    }
    let Ok(raw) = serde_json::from_slice::<Value>(body) else {
        return Vec::new();
    };
    let mut observations = Vec::new();
    if let Some(summary) = raw.get("usage")
        && let Some(observation) = window(summary, "weekly_limit".into(), Some(WEEK_SECONDS))
    {
        observations.push(observation);
    }
    for (index, record) in raw
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
        let seconds = record.get("window").and_then(window_seconds);
        let window_key = label
            .map(|label| crate::shared::quota::slug(label, ""))
            .filter(|key| !key.is_empty())
            .unwrap_or_else(|| format!("limit_{index}"));
        if let Some(observation) = window(detail, window_key, seconds) {
            observations.push(observation);
        }
    }
    observations
}

fn window(record: &Value, window_key: String, seconds: Option<i64>) -> Option<QuotaObservation> {
    let used = record.get("used").and_then(crate::shared::quota::decimal);
    let limit = record.get("limit").and_then(crate::shared::quota::decimal);
    if used.is_none() && limit.is_none() {
        return None;
    }
    let period_end = text(record.get("resetTime")).and_then(crate::shared::quota::iso_to_unix);
    Some(QuotaObservation {
        unit: None,
        reset_behavior: gproxy_channel_api::QuotaResetBehavior::Periodic,
        scope: gproxy_channel_api::QuotaScope::All,
        sample: None,
        window_key,
        label: None,
        period_start: period_end
            .zip(seconds.filter(|seconds| *seconds > 0))
            .map(|(end, seconds)| end - seconds),
        period_end,
        used_percent: limit.and_then(|limit| {
            crate::shared::quota::percent_used(used.unwrap_or(Decimal::ZERO), limit)
        }),
        upstream_used: used,
        upstream_limit: limit,
    })
}

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

fn text(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{WEEK_SECONDS, parse_probe};

    #[test]
    fn weekly_summary_and_limits_become_observations() {
        let body = br#"{
          "usage":{"used":"40","limit":"1000","resetTime":"2030-01-08T00:00:00Z"},
          "limits":[{"name":"Five hour","window":{"duration":300,"timeUnit":"TIME_UNIT_MINUTE"},
                     "detail":{"used":"5","limit":"100","resetTime":"2030-01-01T05:00:00Z"}}]
        }"#;
        let observed = parse_probe(http::StatusCode::OK, body);
        assert_eq!(observed.len(), 2);
        assert_eq!(observed[0].window_key, "weekly_limit");
        assert_eq!(observed[0].used_percent, Some("4".parse().unwrap()));
        assert_eq!(observed[0].upstream_limit, Some("1000".parse().unwrap()));
        let end = observed[0].period_end.unwrap();
        assert_eq!(observed[0].period_start, Some(end - WEEK_SECONDS));
        assert_eq!(observed[1].window_key, "five_hour");
        assert_eq!(observed[1].used_percent, Some("5".parse().unwrap()));
        let end = observed[1].period_end.unwrap();
        assert_eq!(observed[1].period_start, Some(end - 5 * 60 * 60));
    }
}
