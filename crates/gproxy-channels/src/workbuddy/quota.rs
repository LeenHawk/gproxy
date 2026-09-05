//! WorkBuddy (Tencent Copilot) quota. Enterprise credentials hit
//! `POST /v2/billing/meter/get-enterprise-user-usage` with `{}`; personal
//! ones hit `POST /v2/billing/meter/get-user-resource` with the captured
//! plugin body (`ProductCode` `p_tcaca`, a 101-year package window, statuses
//! 0/3). The Accounts list nests at varying depths depending on gateway
//! wrapping; counters arrive as numbers or numeric strings.

use bytes::Bytes;
use gproxy_channel_api::{ChannelError, QuotaObservation};
use http::header::{CONTENT_TYPE, HeaderValue};
use rust_decimal::Decimal;
use serde_json::{Value, json};

pub(super) fn probe_request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<http::Request<Bytes>>, ChannelError> {
    let enterprise = super::auth::field(secret, "enterprise_id").is_some();
    let path = if enterprise {
        "/v2/billing/meter/get-enterprise-user-usage"
    } else {
        "/v2/billing/meter/get-user-resource"
    };
    let uri = crate::shared::http::join(super::auth::base_url(settings), path, None)?;
    let body = if enterprise {
        Bytes::from_static(b"{}")
    } else {
        let now = unix_now();
        Bytes::from(
            json!({
                "PageNumber": 1,
                "PageSize": 100,
                "ProductCode": "p_tcaca",
                "Status": [0, 3],
                "PackageEndTimeRangeBegin": format_time(now),
                "PackageEndTimeRangeEnd": format_time(now.saturating_add(101 * 365 * 86_400)),
            })
            .to_string(),
        )
    };
    let mut headers = http::HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    super::auth::apply(&mut headers, secret)?;
    super::identity::apply(&mut headers, secret)?;
    let mut request = http::Request::post(uri)
        .body(body)
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
    if let Some(accounts) = raw
        .pointer("/data/Response/Data/Accounts")
        .or_else(|| raw.pointer("/data/data/Response/Data/Accounts"))
        .or_else(|| raw.pointer("/Response/Data/Accounts"))
        .and_then(Value::as_array)
    {
        return personal(accounts);
    }
    let data = raw
        .pointer("/data/data")
        .or_else(|| raw.get("data"))
        .unwrap_or(&raw);
    enterprise(data).into_iter().collect()
}

fn personal(accounts: &[Value]) -> Vec<QuotaObservation> {
    accounts
        .iter()
        .enumerate()
        .map(|(index, resource)| {
            let limit = number(resource, "CycleCapacitySizePrecise").unwrap_or(Decimal::ZERO);
            let left = number(resource, "CycleCapacityRemainPrecise").unwrap_or(Decimal::ZERO);
            let used = (limit - left).max(Decimal::ZERO);
            QuotaObservation {
                unit: None,
                reset_behavior: gproxy_channel_api::QuotaResetBehavior::Periodic,
                scope: gproxy_channel_api::QuotaScope::All,
                sample: None,
                window_key: string(resource, "PackageCode")
                    .or_else(|| string(resource, "ResourceId"))
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("resource_{index}")),
                label: None,
                period_start: None,
                period_end: string(resource, "CycleEndTime")
                    .and_then(crate::shared::quota::iso_to_unix),
                used_percent: crate::shared::quota::percent_used(used, limit),
                upstream_used: Some(used),
                upstream_limit: Some(limit),
            }
        })
        .collect()
}

fn enterprise(data: &Value) -> Option<QuotaObservation> {
    let limit = number(data, "limitNum")?;
    let used = number(data, "credit").unwrap_or(Decimal::ZERO);
    Some(QuotaObservation {
        unit: None,
        reset_behavior: gproxy_channel_api::QuotaResetBehavior::Periodic,
        scope: gproxy_channel_api::QuotaScope::All,
        sample: None,
        window_key: "enterprise".into(),
        label: None,
        period_start: None,
        period_end: string(data, "cycleResetTime").and_then(crate::shared::quota::iso_to_unix),
        used_percent: crate::shared::quota::percent_used(used, limit),
        upstream_used: Some(used),
        upstream_limit: Some(limit),
    })
}

fn number(value: &Value, key: &str) -> Option<Decimal> {
    value.get(key).and_then(crate::shared::quota::decimal)
}

fn string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}

fn unix_now() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_secs()
        .try_into()
        .expect("Unix seconds fit i64")
}

/// `YYYY-MM-DD HH:MM:SS`, the format the billing endpoint expects.
fn format_time(timestamp: i64) -> String {
    let days = timestamp.div_euclid(86_400);
    let seconds = timestamp.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds / 3_600;
    let minute = seconds % 3_600 / 60;
    let second = seconds % 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}")
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::parse_probe;

    #[test]
    fn personal_accounts_become_observations() {
        let body = br#"{"data":{"Response":{"Data":{"Accounts":[
          {"PackageCode":"pkg_basic","CycleCapacitySizePrecise":1000,
           "CycleCapacityRemainPrecise":"250.5","CycleEndTime":"2026-09-01T00:00:00Z"}
        ]}}}}"#;
        let observed = parse_probe(http::StatusCode::OK, body);
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].window_key, "pkg_basic");
        assert_eq!(observed[0].upstream_used, Some("749.5".parse().unwrap()));
        assert_eq!(observed[0].upstream_limit, Some("1000".parse().unwrap()));
        assert_eq!(observed[0].used_percent, Some("74.95".parse().unwrap()));
        assert_eq!(observed[0].period_end, Some(1_788_220_800));
    }

    #[test]
    fn enterprise_payload_becomes_single_observation() {
        let body = br#"{"data":{"data":{"limitNum":500,"credit":"120"}}}"#;
        let observed = parse_probe(http::StatusCode::OK, body);
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].window_key, "enterprise");
        assert_eq!(observed[0].used_percent, Some("24".parse().unwrap()));
        assert_eq!(observed[0].upstream_limit, Some("500".parse().unwrap()));
    }

    #[test]
    fn billing_time_format_matches_the_captured_plugin_body() {
        assert_eq!(super::format_time(1_767_225_600), "2026-01-01 00:00:00");
        assert_eq!(super::format_time(1_788_220_800), "2026-09-01 00:00:00");
    }
}
