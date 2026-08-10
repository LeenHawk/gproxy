//! Grok Build per-credential usage - `GET /billing?format=credits` on the
//! CLI chat proxy. The local Grok 0.2.93 binary's `/usage show` command routes
//! through `xai_grok_shell::extensions::billing` to this endpoint; the
//! OpenAI-compatible `https://api.x.ai/v1` API does not expose it.

use bytes::Bytes;
use http::{HeaderMap, Method, Request, StatusCode};
use serde::Deserialize;
use serde_json::Value;

use super::auth;
use crate::channel::ChannelError;
use crate::channel::http_util::{build_request, join_url};
use crate::channel::usage::{
    UsageCredits, UsageSnapshot, UsageWindow, UsageWindowBoundaryConfidence,
    UsageWindowBoundarySource, UsageWindowDescriptor, UsageWindowMeter, UsageWindowScope,
};
use crate::channel::usage_descriptor::iso_to_unix;

const DEFAULT_USAGE_BASE_URL: &str = "https://cli-chat-proxy.grok.com/v1";

pub(super) fn request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<Request<Bytes>>, ChannelError> {
    let base = usage_base_url(settings, secret);
    let uri = join_url(base, "/billing", Some("format=credits"))?;
    let mut req = build_request(Method::GET, uri, HeaderMap::new(), Bytes::new())?;
    auth::apply(&mut req, secret, auth::AcceptMode::Json, None)?;
    if let Some(user_id) = auth::user_id(secret) {
        let user_id = http::HeaderValue::from_str(user_id)
            .map_err(|e| ChannelError::Build(format!("bad x-userid: {e}")))?;
        req.headers_mut().insert("x-userid", user_id);
    }
    Ok(Some(req))
}

pub(super) fn parse(status: StatusCode, body: &Bytes) -> Option<UsageSnapshot> {
    if !status.is_success() {
        return None;
    }
    let raw: Value = serde_json::from_slice(body).ok()?;
    let config = serde_json::from_value::<BillingResponse>(raw.clone())
        .ok()
        .and_then(|payload| payload.config)
        .or_else(|| serde_json::from_value::<BillingConfig>(raw.clone()).ok())?;

    let reset = config
        .current_period
        .as_ref()
        .and_then(|period| period.end.clone())
        .or_else(|| config.billing_period_end.clone());

    let mut windows = Vec::new();
    if config.credit_usage_percent.is_some()
        || config.current_period.is_some()
        || config.monthly_limit.is_some()
        || config.used.is_some()
        || config.billing_period_end.is_some()
    {
        let name = config
            .current_period
            .as_ref()
            .and_then(|period| period.period_type.as_deref())
            .map(period_window_name)
            .unwrap_or("usage");
        let mut window = UsageWindow {
            name: name.to_owned(),
            used_percent: Some(included_usage_percent(&config).unwrap_or(0.0)),
            used: config.used.as_ref().and_then(MoneyValue::number),
            limit: config.monthly_limit.as_ref().and_then(MoneyValue::number),
            ..Default::default()
        };
        window = with_period(window, reset.as_deref());
        windows.push(window);
    }
    for product in config.product_usage.as_deref().unwrap_or_default() {
        let Some(percent) = product.usage_percent else {
            continue;
        };
        let label = product
            .product
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("Product");
        let key = non_empty_key(label, "product");
        windows.push(with_period(
            UsageWindow::percent(format!("product:{key}"), percent).label(label),
            reset.as_deref(),
        ));
    }

    let credits = credits(&config);

    Some(UsageSnapshot {
        plan: None,
        windows,
        credits,
        rate_limit_reset_credits: None,
        raw,
    })
}

pub(super) fn describe(snapshot: &UsageSnapshot, index: usize) -> UsageWindowDescriptor {
    let Some(window) = snapshot.windows.get(index) else {
        return UsageWindowDescriptor::from_window(&UsageWindow {
            name: format!("window_{index}"),
            ..Default::default()
        });
    };
    let scope = window
        .name
        .strip_prefix("product:")
        .map(|product| UsageWindowScope::Feature {
            feature: product.to_owned(),
        })
        .unwrap_or(UsageWindowScope::All);
    let descriptor = UsageWindowDescriptor::from_window(window)
        .scope(scope)
        .meter(UsageWindowMeter::Credits);
    let config = serde_json::from_value::<BillingResponse>(snapshot.raw.clone())
        .ok()
        .and_then(|payload| payload.config)
        .or_else(|| serde_json::from_value::<BillingConfig>(snapshot.raw.clone()).ok());
    let start = config
        .as_ref()
        .and_then(|config| {
            config
                .current_period
                .as_ref()
                .and_then(|period| period.start.as_deref())
                .or(config.billing_period_start.as_deref())
        })
        .and_then(iso_to_unix);
    match start {
        Some(start) => descriptor.period_start(
            start,
            UsageWindowBoundarySource::Upstream,
            UsageWindowBoundaryConfidence::Exact,
        ),
        None => descriptor,
    }
}

fn usage_base_url<'a>(settings: &'a Value, secret: &'a Value) -> &'a str {
    settings
        .get("usage_base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            secret
                .get("usage_base_url")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
        })
        .unwrap_or(DEFAULT_USAGE_BASE_URL)
}

fn with_period(mut window: UsageWindow, reset: Option<&str>) -> UsageWindow {
    if let Some(reset) = reset {
        window = window.resets_iso(reset);
    }
    window
}

fn period_window_name(period_type: &str) -> &'static str {
    if period_type.contains("WEEKLY") {
        "weekly_limit"
    } else if period_type.contains("MONTHLY") {
        "monthly_limit"
    } else {
        "usage"
    }
}

fn included_usage_percent(config: &BillingConfig) -> Option<f64> {
    if let Some(percent) = config.credit_usage_percent {
        return Some(percent.clamp(0.0, 100.0));
    }

    let limit = config.monthly_limit.as_ref().and_then(MoneyValue::number)?;
    if limit <= 0.0 {
        return None;
    }
    let used = config.used.as_ref().and_then(MoneyValue::number)?;
    Some((used / limit * 100.0).clamp(0.0, 100.0))
}

fn credits(config: &BillingConfig) -> Option<UsageCredits> {
    let balance_number = config.prepaid_balance.as_ref().and_then(MoneyValue::number);
    let balance = config
        .prepaid_balance
        .as_ref()
        .and_then(MoneyValue::display);
    let used = config.on_demand_used.as_ref().and_then(MoneyValue::number);
    let cap = config.on_demand_cap.as_ref().and_then(MoneyValue::number);
    let has_prepaid = balance_number.is_some_and(|value| value.abs() > f64::EPSILON);
    let has_pay_as_you_go = cap.is_some_and(|value| value.abs() > f64::EPSILON);
    if !has_prepaid && !has_pay_as_you_go {
        return None;
    }
    Some(UsageCredits {
        has_credits: balance_number.map(|value| value > 0.0),
        balance,
        used_credits: used,
        monthly_limit: cap,
        ..Default::default()
    })
}

fn key(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

fn non_empty_key(value: &str, fallback: &str) -> String {
    let key = key(value);
    if key.is_empty() {
        fallback.to_string()
    } else {
        key
    }
}

#[derive(Deserialize)]
struct BillingResponse {
    config: Option<BillingConfig>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BillingConfig {
    current_period: Option<BillingPeriod>,
    credit_usage_percent: Option<f64>,
    monthly_limit: Option<MoneyValue>,
    used: Option<MoneyValue>,
    on_demand_cap: Option<MoneyValue>,
    on_demand_used: Option<MoneyValue>,
    product_usage: Option<Vec<ProductUsage>>,
    prepaid_balance: Option<MoneyValue>,
    billing_period_start: Option<String>,
    billing_period_end: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BillingPeriod {
    #[serde(rename = "type")]
    period_type: Option<String>,
    start: Option<String>,
    end: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductUsage {
    product: Option<String>,
    usage_percent: Option<f64>,
}

#[derive(Deserialize)]
struct MoneyValue {
    val: Option<Value>,
}

impl MoneyValue {
    fn number(&self) -> Option<f64> {
        match self.val.as_ref()? {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.parse().ok(),
            _ => None,
        }
    }

    fn display(&self) -> Option<String> {
        match self.val.as_ref()? {
            Value::Number(n) => Some(n.to_string()),
            Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn billing_request_uses_oauth_proxy_identity() {
        let secret = json!({ "access_token": "oauth-token", "sub": "user-1" });
        let request = request(&secret, &Value::Null).unwrap().unwrap();

        assert_eq!(
            request.uri().to_string(),
            "https://cli-chat-proxy.grok.com/v1/billing?format=credits"
        );
        assert_eq!(request.headers()["authorization"], "Bearer oauth-token");
        assert_eq!(request.headers()["x-xai-token-auth"], "xai-grok-cli");
        assert_eq!(request.headers()["x-userid"], "user-1");
    }

    #[test]
    fn parses_billing_credits_payload() {
        let body = Bytes::from_static(
            br#"{
              "config": {
                "currentPeriod": {
                  "type": "USAGE_PERIOD_TYPE_WEEKLY",
                  "start": "2026-07-08T18:30:33.133982+00:00",
                  "end": "2026-07-15T18:30:33.133982+00:00"
                },
                "creditUsagePercent": 2.0,
                "onDemandCap": {"val": 25},
                "onDemandUsed": {"val": 3.5},
                "productUsage": [
                  {"product": "Api", "usagePercent": 2.0}
                ],
                "isUnifiedBillingUser": true,
                "prepaidBalance": {"val": 0},
                "billingPeriodStart": "2026-07-08T18:30:33.133982+00:00",
                "billingPeriodEnd": "2026-07-15T18:30:33.133982+00:00"
              }
            }"#,
        );

        let snap = parse(StatusCode::OK, &body).expect("snapshot");
        assert_eq!(snap.windows.len(), 2);
        assert_eq!(snap.windows[0].name, "weekly_limit");
        assert_eq!(snap.windows[0].used_percent, Some(2.0));
        assert_eq!(
            snap.windows[0].resets_at.as_deref(),
            Some("2026-07-15T18:30:33.133982+00:00")
        );
        assert_eq!(snap.windows[1].name, "product:api");
        let credits = snap.credits.expect("credits");
        assert_eq!(credits.balance.as_deref(), Some("0"));
        assert_eq!(credits.used_credits, Some(3.5));
        assert_eq!(credits.monthly_limit, Some(25.0));
    }

    #[test]
    fn zero_credit_payload_matches_grok_build_fallback() {
        let body = Bytes::from_static(
            br#"{
              "currentPeriod": {
                "type": "USAGE_PERIOD_TYPE_WEEKLY",
                "start": "2026-08-08T00:00:00+00:00",
                "end": "2026-08-15T00:00:00+00:00"
              },
              "creditUsagePercent": null,
              "productUsage": null,
              "onDemandCap": {"val": 0},
              "onDemandUsed": {"val": 0},
              "prepaidBalance": {"val": 0}
            }"#,
        );

        let snap = parse(StatusCode::OK, &body).expect("snapshot");
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].name, "weekly_limit");
        assert_eq!(snap.windows[0].used_percent, Some(0.0));
        assert_eq!(
            snap.windows[0].resets_at.as_deref(),
            Some("2026-08-15T00:00:00+00:00")
        );
        assert!(snap.credits.is_none());
    }

    #[test]
    fn legacy_payload_derives_percent_from_used_and_limit() {
        let body = Bytes::from_static(
            br#"{
              "monthlyLimit": {"val": 2000},
              "used": {"val": 500},
              "billingPeriodEnd": "2026-09-01T00:00:00+00:00"
            }"#,
        );

        let snap = parse(StatusCode::OK, &body).expect("snapshot");
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].name, "usage");
        assert_eq!(snap.windows[0].used_percent, Some(25.0));
        assert_eq!(
            snap.windows[0].resets_at.as_deref(),
            Some("2026-09-01T00:00:00+00:00")
        );
    }
}
