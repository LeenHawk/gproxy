//! Grok Build per-credential usage - `GET /billing?format=credits` on the
//! CLI chat proxy. The local Grok 0.2.93 binary's `/usage show` command routes
//! through `xai_grok_shell::extensions::billing` to this endpoint; the
//! OpenAI-compatible `https://api.x.ai/v1` API does not expose it.

use bytes::Bytes;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderValue};
use http::{HeaderMap, Method, Request, StatusCode};
use serde::Deserialize;
use serde_json::Value;

use super::auth;
use crate::channel::ChannelError;
use crate::channel::http_util::{build_request, join_url};
use crate::channel::usage::{UsageCredits, UsageSnapshot, UsageWindow};

const DEFAULT_USAGE_BASE_URL: &str = "https://cli-chat-proxy.grok.com/v1";

pub(super) fn request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<Request<Bytes>>, ChannelError> {
    let token = auth::bearer_token(secret)?;
    let base = usage_base_url(settings, secret);
    let uri = join_url(base, "/billing", Some("format=credits"))?;
    let mut req = build_request(Method::GET, uri, HeaderMap::new(), Bytes::new())?;
    let bearer = HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|e| ChannelError::InvalidCredential(format!("bad bearer token: {e}")))?;
    let h = req.headers_mut();
    h.insert(AUTHORIZATION, bearer);
    h.insert(ACCEPT, HeaderValue::from_static("application/json"));
    h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
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
    if let Some(percent) = config.credit_usage_percent {
        windows.push(with_period(
            UsageWindow::percent("credits", percent).label("Credits"),
            reset.as_deref(),
        ));
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

fn credits(config: &BillingConfig) -> Option<UsageCredits> {
    let balance = config
        .prepaid_balance
        .as_ref()
        .and_then(MoneyValue::display);
    let used = config.on_demand_used.as_ref().and_then(MoneyValue::number);
    let cap = config.on_demand_cap.as_ref().and_then(MoneyValue::number);
    if balance.is_none() && used.is_none() && cap.is_none() {
        return None;
    }
    Some(UsageCredits {
        has_credits: config
            .prepaid_balance
            .as_ref()
            .and_then(MoneyValue::number)
            .map(|v| v > 0.0),
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
    on_demand_cap: Option<MoneyValue>,
    on_demand_used: Option<MoneyValue>,
    product_usage: Option<Vec<ProductUsage>>,
    prepaid_balance: Option<MoneyValue>,
    billing_period_end: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BillingPeriod {
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
        assert_eq!(snap.windows[0].name, "credits");
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
}
