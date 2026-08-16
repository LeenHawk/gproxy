//! Kimi Code subscription quota from `GET /usages`.

use bytes::Bytes;
use http::{HeaderMap, Method, Request, StatusCode};
use serde_json::Value;

use super::auth;
use crate::channel::ChannelError;
use crate::channel::http_util::{build_request, join_url};
use crate::channel::usage::{
    UsageCredits, UsageSnapshot, UsageWindow, UsageWindowDescriptor, UsageWindowMeter,
    UsageWindowScope,
};

const WEEK_SECONDS: i64 = 7 * 24 * 60 * 60;
const FIXED_POINT_CENTS: f64 = 1_000_000.0;

pub(super) fn request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<Request<Bytes>>, ChannelError> {
    let uri = join_url(auth::base_url(settings, secret), "/usages", None)?;
    let mut request = build_request(Method::GET, uri, HeaderMap::new(), Bytes::new())?;
    auth::apply(&mut request, secret, false)?;
    Ok(Some(request))
}

pub(super) fn parse(status: StatusCode, body: &Bytes) -> Option<UsageSnapshot> {
    if !status.is_success() {
        return None;
    }
    let raw: Value = serde_json::from_slice(body).ok()?;
    let mut windows = Vec::new();
    if let Some(summary) = raw.get("usage").and_then(Value::as_object)
        && let Some(window) = usage_window(summary, "weekly_limit", Some(WEEK_SECONDS))
    {
        windows.push(window);
    }
    if let Some(limits) = raw.get("limits").and_then(Value::as_array) {
        for (index, limit) in limits.iter().enumerate() {
            let Some(record) = limit.as_object() else {
                continue;
            };
            let Some(detail) = record.get("detail").and_then(Value::as_object) else {
                continue;
            };
            let label = text(record.get("name")).or_else(|| text(detail.get("name")));
            let seconds = record.get("window").and_then(window_seconds);
            let generated = label
                .map(key)
                .filter(|key| !key.is_empty())
                .unwrap_or_else(|| format!("limit_{index}"));
            if let Some(mut window) = usage_window(detail, &generated, seconds) {
                window.label = label.map(ToOwned::to_owned);
                windows.push(window);
            }
        }
    }
    Some(UsageSnapshot {
        windows,
        credits: booster_credits(&raw),
        raw,
        ..Default::default()
    })
}

pub(super) fn describe(snapshot: &UsageSnapshot, index: usize) -> UsageWindowDescriptor {
    let Some(window) = snapshot.windows.get(index) else {
        return UsageWindowDescriptor::from_window(&UsageWindow {
            name: format!("window_{index}"),
            ..Default::default()
        });
    };
    let scope = if window.name == "weekly_limit" {
        UsageWindowScope::All
    } else {
        UsageWindowScope::Feature {
            feature: window.label.clone().unwrap_or_else(|| window.name.clone()),
        }
    };
    UsageWindowDescriptor::from_window(window)
        .scope(scope)
        .meter(UsageWindowMeter::Opaque)
}

fn usage_window(
    record: &serde_json::Map<String, Value>,
    name: &str,
    window_seconds: Option<i64>,
) -> Option<UsageWindow> {
    let parsed_used = number(record.get("used"));
    let parsed_limit = number(record.get("limit"));
    if parsed_used.is_none() && parsed_limit.is_none() {
        return None;
    }
    let used = parsed_used.unwrap_or(0.0);
    let limit = parsed_limit.unwrap_or(0.0);
    let mut window = UsageWindow {
        name: name.into(),
        label: text(record.get("name")).map(ToOwned::to_owned),
        used: Some(used),
        limit: Some(limit),
        used_percent: (limit > 0.0).then(|| (used / limit * 100.0).clamp(0.0, 100.0)),
        window_seconds,
        ..Default::default()
    };
    if let Some(reset) = text(record.get("resetTime")) {
        window.resets_at = Some(reset.to_owned());
        window.resets_at_unix = crate::channel::usage_descriptor::iso_to_unix(reset);
    }
    Some(window)
}

fn window_seconds(value: &Value) -> Option<i64> {
    let record = value.as_object()?;
    let duration = integer(record.get("duration"))?;
    let multiplier = match text(record.get("timeUnit"))? {
        "TIME_UNIT_MINUTE" => 60,
        "TIME_UNIT_HOUR" => 60 * 60,
        "TIME_UNIT_DAY" => 24 * 60 * 60,
        "TIME_UNIT_WEEK" => WEEK_SECONDS,
        _ => return None,
    };
    duration.checked_mul(multiplier)
}

fn booster_credits(raw: &Value) -> Option<UsageCredits> {
    let wallet = raw.get("boosterWallet")?.as_object()?;
    let balance = wallet.get("balance")?.as_object()?;
    if text(balance.get("type"))? != "BOOSTER" {
        return None;
    }
    let total = number(balance.get("amount"))? / FIXED_POINT_CENTS;
    if total <= 0.0 {
        return None;
    }
    let remaining = number(balance.get("amountLeft")).unwrap_or(0.0) / FIXED_POINT_CENTS;
    let monthly_limit = wallet
        .get("monthlyChargeLimit")
        .and_then(|value| value.get("priceInCents"))
        .and_then(|value| number(Some(value)))
        .map(|cents| cents / 100.0);
    let monthly_used = wallet
        .get("monthlyUsed")
        .and_then(|value| value.get("priceInCents"))
        .and_then(|value| number(Some(value)))
        .map(|cents| cents / 100.0);
    let currency = wallet
        .get("monthlyChargeLimit")
        .and_then(|value| text(value.get("currency")))
        .or_else(|| {
            wallet
                .get("monthlyUsed")
                .and_then(|value| text(value.get("currency")))
        })
        .unwrap_or("USD");
    let monthly_limit_enabled = wallet
        .get("monthlyChargeLimitEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Some(UsageCredits {
        has_credits: Some(remaining > 0.0),
        balance: Some(format!("{:.2}", remaining / 100.0)),
        used_credits: monthly_used,
        monthly_limit,
        unlimited: Some(!monthly_limit_enabled),
        currency: Some(currency.into()),
    })
}

fn text(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(value) => value.as_f64(),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

fn integer(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(value) => value.as_i64(),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

fn key(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_owned()
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn parses_official_usage_shape() {
        let body = Bytes::from_static(
            br#"{
              "usage":{"used":"40","limit":"1000","resetTime":"2030-01-08T00:00:00Z"},
              "limits":[{"name":"Five hour","window":{"duration":300,"timeUnit":"TIME_UNIT_MINUTE"},"detail":{"used":"5","limit":"100","resetTime":"2030-01-01T05:00:00Z"}}]
            }"#,
        );
        let snapshot = parse(StatusCode::OK, &body).unwrap();
        assert_eq!(snapshot.windows.len(), 2);
        assert_eq!(snapshot.windows[0].name, "weekly_limit");
        assert_eq!(snapshot.windows[0].window_seconds, Some(WEEK_SECONDS));
        assert_eq!(snapshot.windows[1].window_seconds, Some(5 * 60 * 60));
        assert_eq!(snapshot.windows[1].used_percent, Some(5.0));
    }
}
