use bytes::Bytes;
use http::{HeaderMap, Method, Request, StatusCode};
use serde_json::{Value, json};

use super::auth;
use crate::channel::http_util::{build_request, join_url};
use crate::channel::{ChannelError, UsageSnapshot, UsageWindow};

pub(super) fn request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<Request<Bytes>>, ChannelError> {
    let enterprise = auth::field(secret, "enterprise_id").is_some();
    let path = if enterprise {
        "/v2/billing/meter/get-enterprise-user-usage"
    } else {
        "/v2/billing/meter/get-user-resource"
    };
    let uri = join_url(auth::base_url(settings), path, None)?;
    let body = if enterprise {
        Bytes::from_static(b"{}")
    } else {
        let now = crate::util::time::unix_now();
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
    let mut request = build_request(Method::POST, uri, HeaderMap::new(), body)?;
    request.headers_mut().insert(
        http::header::CONTENT_TYPE,
        "application/json".parse().expect("static"),
    );
    auth::apply(&mut request, secret)?;
    Ok(Some(request))
}

pub(super) fn parse(status: StatusCode, body: &Bytes) -> Option<UsageSnapshot> {
    if !status.is_success() {
        return None;
    }
    let raw: Value = serde_json::from_slice(body).ok()?;
    if let Some(accounts) = raw
        .pointer("/data/Response/Data/Accounts")
        .or_else(|| raw.pointer("/data/data/Response/Data/Accounts"))
        .or_else(|| raw.pointer("/Response/Data/Accounts"))
        .and_then(Value::as_array)
    {
        return Some(personal(raw.clone(), accounts));
    }
    let data = raw
        .pointer("/data/data")
        .or_else(|| raw.get("data"))
        .unwrap_or(&raw);
    enterprise(raw.clone(), data)
}

fn personal(raw: Value, accounts: &[Value]) -> UsageSnapshot {
    let windows = accounts
        .iter()
        .enumerate()
        .map(|(index, resource)| {
            let limit = number(resource, "CycleCapacitySizePrecise").unwrap_or(0.0);
            let left = number(resource, "CycleCapacityRemainPrecise").unwrap_or(0.0);
            let name = string(resource, "PackageCode")
                .or_else(|| string(resource, "ResourceId"))
                .map(str::to_string)
                .unwrap_or_else(|| format!("resource_{index}"));
            let reset = string(resource, "CycleEndTime").map(str::to_string);
            UsageWindow {
                name,
                used: Some((limit - left).max(0.0)),
                limit: Some(limit),
                used_percent: percent(limit - left, limit),
                resets_at: reset,
                ..Default::default()
            }
        })
        .collect();
    UsageSnapshot {
        windows,
        raw,
        ..Default::default()
    }
}

fn enterprise(raw: Value, data: &Value) -> Option<UsageSnapshot> {
    let limit = number(data, "limitNum")?;
    let used = number(data, "credit").unwrap_or(0.0);
    let reset = data.get("cycleResetTime").and_then(|value| match value {
        Value::String(value) => Some(value.clone()),
        _ => None,
    });
    Some(UsageSnapshot {
        windows: vec![UsageWindow {
            name: "enterprise".into(),
            used: Some(used),
            limit: Some(limit),
            used_percent: percent(used, limit),
            resets_at: reset,
            ..Default::default()
        }],
        raw,
        ..Default::default()
    })
}

fn number(value: &Value, key: &str) -> Option<f64> {
    match value.get(key)? {
        Value::Number(number) => number.as_f64(),
        Value::String(number) => number.parse().ok(),
        _ => None,
    }
}

fn string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

fn percent(used: f64, limit: f64) -> Option<f64> {
    (limit > 0.0).then(|| (used / limit * 100.0).clamp(0.0, 100.0))
}

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
