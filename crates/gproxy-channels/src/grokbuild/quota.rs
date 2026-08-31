//! Grok Build usage — `GET {usage-base}/billing?format=credits` on the CLI
//! chat proxy (the OpenAI-compatible api.x.ai surface does not expose it).
//! The payload arrives wrapped in `config` or bare; period boundaries are
//! upstream-exact ISO timestamps, never derived. Money values nest as
//! `{"val": <number|numeric string>}`.

use bytes::Bytes;
use gproxy_channel_api::{ChannelError, QuotaObservation};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;

const DEFAULT_USAGE_BASE_URL: &str = "https://cli-chat-proxy.grok.com/v1";

pub(super) fn probe_request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<http::Request<Bytes>>, ChannelError> {
    let base = super::auth::field(settings, "usage_base_url")
        .or_else(|| super::auth::field(secret, "usage_base_url"))
        .unwrap_or(DEFAULT_USAGE_BASE_URL);
    let uri = crate::shared::http::join(base, "/billing", Some("format=credits"))?;
    let mut headers = http::HeaderMap::new();
    super::auth::apply(&mut headers, secret, false, false, None)?;
    if let Some(user) = super::auth::field(secret, "sub") {
        headers.insert(
            http::HeaderName::from_static("x-userid"),
            http::HeaderValue::from_str(user)
                .map_err(|error| ChannelError::Secret(format!("bad x-userid: {error}")))?,
        );
    }
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
    let config = serde_json::from_value::<BillingResponse>(raw.clone())
        .ok()
        .and_then(|payload| payload.config)
        .or_else(|| serde_json::from_value::<BillingConfig>(raw).ok());
    let Some(config) = config else {
        return Vec::new();
    };
    let period_end = config
        .current_period
        .as_ref()
        .and_then(|period| period.end.as_deref())
        .or(config.billing_period_end.as_deref())
        .and_then(crate::shared::quota::iso_to_unix);
    let period_start = config
        .current_period
        .as_ref()
        .and_then(|period| period.start.as_deref())
        .or(config.billing_period_start.as_deref())
        .and_then(crate::shared::quota::iso_to_unix);
    let mut observations = Vec::new();
    if config.credit_usage_percent.is_some()
        || config.current_period.is_some()
        || config.monthly_limit.is_some()
        || config.used.is_some()
        || config.billing_period_end.is_some()
    {
        let window_key = config
            .current_period
            .as_ref()
            .and_then(|period| period.period_type.as_deref())
            .map(period_window_key)
            .unwrap_or("usage");
        observations.push(QuotaObservation {
            window_key: window_key.to_owned(),
            period_start,
            period_end,
            used_percent: Some(included_percent(&config).unwrap_or(Decimal::ZERO)),
            upstream_used: config.used.as_ref().and_then(MoneyValue::number),
            upstream_limit: config.monthly_limit.as_ref().and_then(MoneyValue::number),
        });
    }
    for product in config.product_usage.iter().flatten() {
        let Some(percent) = product.usage_percent else {
            continue;
        };
        let label = product
            .product
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Product");
        observations.push(QuotaObservation {
            window_key: format!("product:{}", crate::shared::quota::slug(label, "product")),
            period_start,
            period_end,
            used_percent: Decimal::try_from(percent).ok(),
            upstream_used: None,
            upstream_limit: None,
        });
    }
    observations
}

fn period_window_key(period_type: &str) -> &'static str {
    if period_type.contains("WEEKLY") {
        "weekly_limit"
    } else if period_type.contains("MONTHLY") {
        "monthly_limit"
    } else {
        "usage"
    }
}

fn included_percent(config: &BillingConfig) -> Option<Decimal> {
    if let Some(percent) = config.credit_usage_percent {
        return Decimal::try_from(percent.clamp(0.0, 100.0)).ok();
    }
    let limit = config.monthly_limit.as_ref().and_then(MoneyValue::number)?;
    let used = config.used.as_ref().and_then(MoneyValue::number)?;
    crate::shared::quota::percent_used(used, limit)
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
    product_usage: Option<Vec<ProductUsage>>,
    billing_period_start: Option<String>,
    billing_period_end: Option<String>,
}

#[derive(Deserialize)]
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
    fn number(&self) -> Option<Decimal> {
        self.val.as_ref().and_then(crate::shared::quota::decimal)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_probe;

    #[test]
    fn wrapped_payload_yields_primary_and_product_windows() {
        let body = br#"{"config":{
          "currentPeriod":{"type":"USAGE_PERIOD_TYPE_WEEKLY",
            "start":"2026-07-08T18:30:33+00:00","end":"2026-07-15T18:30:33+00:00"},
          "creditUsagePercent":2.0,
          "productUsage":[{"product":"Api","usagePercent":2.0}]
        }}"#;
        let observed = parse_probe(http::StatusCode::OK, body);
        assert_eq!(observed.len(), 2);
        assert_eq!(observed[0].window_key, "weekly_limit");
        assert_eq!(observed[0].used_percent, Some("2".parse().unwrap()));
        // Boundaries are upstream-exact, not derived from a window length.
        assert_eq!(observed[0].period_start, Some(1_783_535_433));
        assert_eq!(observed[0].period_end, Some(1_784_140_233));
        assert_eq!(observed[1].window_key, "product:api");
        assert_eq!(observed[1].period_end, observed[0].period_end);
    }

    #[test]
    fn bare_legacy_payload_derives_percent_from_used_and_limit() {
        let body = br#"{
          "monthlyLimit":{"val":2000},
          "used":{"val":500},
          "billingPeriodEnd":"2026-09-01T00:00:00+00:00"
        }"#;
        let observed = parse_probe(http::StatusCode::OK, body);
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].window_key, "usage");
        assert_eq!(observed[0].used_percent, Some("25".parse().unwrap()));
        assert_eq!(observed[0].upstream_used, Some("500".parse().unwrap()));
        assert_eq!(observed[0].upstream_limit, Some("2000".parse().unwrap()));
        assert_eq!(observed[0].period_start, None);
    }
}
