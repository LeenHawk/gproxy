//! What the account has left.
//!
//! `GET {usage base}/billing?format=credits` on the chat proxy; the
//! OpenAI-compatible `api.x.ai` surface does not answer it (v3
//! `grokbuild/quota.rs`). The payload arrives either wrapped in `config` or
//! bare, period boundaries are upstream-exact ISO timestamps and money nests
//! as `{"val": number-or-string}`. Per-product rows report only a percentage.
//!
//! Account periods can stop selection at subscription exhaustion unless paid
//! usage is permitted. Per-product readings remain observations; they do not
//! identify a model scope or a separately renewable subscription allowance.

use super::auth::{Reply, apply};
use super::config::{GrokBuildConfig, ID};
use super::unix_now_ms;
use crate::channel::{
    ChannelError, CredentialContext, CredentialView, OperationFuture, ProviderView, QuotaAllowance,
    QuotaBalance, QuotaDimension, QuotaEntry, QuotaMetric, QuotaModel, QuotaQuery,
    QuotaResetBehavior, QuotaScope, QuotaSnapshot, QuotaSubject, QuotaTracking, QuotaValue,
    QuotaWindow,
};
use crate::channels::shared::compatible::ability::{decimal, require_success, send};
use crate::channels::shared::compatible::http::invalid_response;
use http::{HeaderMap, HeaderName, HeaderValue, Method};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;

/// The window a period type the reply does not name falls back to.
pub const USAGE_DIMENSION: &str = "usage";
/// The account usage page the CLI's `PurchaseCredits` action opens.
pub const TOP_UP_URL: &str = "https://grok.com?_s=usage";

impl QuotaModel for super::GrokBuild {
    fn dimensions(&self, _: ProviderView<'_>, _: CredentialView<'_>) -> Vec<QuotaDimension> {
        [
            (
                "weekly_limit",
                QuotaWindow::Rolling {
                    seconds: 7 * 24 * 60 * 60,
                },
            ),
            ("monthly_limit", QuotaWindow::CalendarMonth),
            (USAGE_DIMENSION, QuotaWindow::Total),
        ]
        .into_iter()
        .map(|(id, window)| QuotaDimension {
            id: id.into(),
            label: None,
            scope: QuotaScope::All,
            operations: None,
            metric: QuotaMetric::Unit("percent".into()),
            window,
            limit: None,
            tracking: QuotaTracking::Reported,
            blocking: true,
        })
        .collect()
    }

    fn allows_paid_usage(&self, credential: CredentialView<'_>, dimension: &str) -> bool {
        credential
            .metadata
            .get("allow_paid_usage")
            .and_then(Value::as_bool)
            == Some(true)
            && matches!(
                dimension,
                "weekly_limit" | "monthly_limit" | USAGE_DIMENSION
            )
    }

    fn classify<'d>(
        &self,
        declared: &'d [QuotaDimension],
        entry: &QuotaEntry,
    ) -> Option<std::borrow::Cow<'d, QuotaDimension>> {
        if entry.source_id == USAGE_DIMENSION {
            let QuotaValue::Window(window) = &entry.value else {
                return None;
            };
            // An unnamed period must carry its own reset; do not invent a
            // month-long block from an unbounded balance-like reading.
            window.period_end_ms?;
        }
        crate::channel::classify_by_id(declared, entry)
    }
}

#[derive(Deserialize)]
struct BillingResponse {
    #[serde(default)]
    config: Option<BillingConfig>,
    #[serde(default)]
    subscription_tier: Option<String>,
    #[serde(default)]
    on_demand_enabled: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BillingConfig {
    #[serde(default)]
    current_period: Option<BillingPeriod>,
    #[serde(default)]
    credit_usage_percent: Option<f64>,
    #[serde(default)]
    monthly_limit: Option<Money>,
    #[serde(default)]
    used: Option<Money>,
    /// Purchased credits, reported in USD cents by the CLI billing API.
    #[serde(default)]
    prepaid_balance: Option<Money>,
    #[serde(default)]
    on_demand_cap: Option<Money>,
    #[serde(default)]
    on_demand_used: Option<Money>,
    #[serde(default)]
    is_unified_billing_user: Option<bool>,
    #[serde(default)]
    history: Vec<BillingHistory>,
    #[serde(default)]
    product_usage: Option<Vec<ProductUsage>>,
    #[serde(default)]
    billing_period_start: Option<String>,
    #[serde(default)]
    billing_period_end: Option<String>,
}

#[derive(Deserialize)]
struct BillingPeriod {
    #[serde(default, rename = "type")]
    period_type: Option<String>,
    #[serde(default)]
    start: Option<String>,
    #[serde(default)]
    end: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BillingHistory {
    #[serde(default)]
    billing_cycle: Option<BillingCycle>,
    #[serde(default)]
    included_used: Option<Money>,
    #[serde(default)]
    on_demand_used: Option<Money>,
    #[serde(default)]
    total_used: Option<Money>,
}

#[derive(Deserialize)]
struct BillingCycle {
    year: i32,
    month: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductUsage {
    #[serde(default)]
    product: Option<String>,
    #[serde(default)]
    usage_percent: Option<f64>,
}

#[derive(Deserialize)]
struct Money {
    #[serde(default)]
    val: Option<Value>,
}

impl Money {
    fn number(&self) -> Option<Decimal> {
        // proto3 JSON omits zero-valued cents, encoding them as `{}`.
        self.val.as_ref().map_or(Some(Decimal::ZERO), decimal)
    }
}

fn iso_to_ms(value: &str) -> Option<i64> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|stamp| stamp.unix_timestamp() * 1_000)
}

fn period_window_key(period_type: &str) -> &'static str {
    if period_type.contains("WEEKLY") {
        "weekly_limit"
    } else if period_type.contains("MONTHLY") {
        "monthly_limit"
    } else {
        USAGE_DIMENSION
    }
}

/// A lowercase, underscore-joined key from a product name.
fn slug(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    for character in label.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let out = out.trim_matches('_').to_owned();
    if out.is_empty() {
        "product".into()
    } else {
        out
    }
}

fn used_percent(config: &BillingConfig) -> Option<Decimal> {
    if let Some(percent) = config.credit_usage_percent {
        return Decimal::try_from(percent.clamp(0.0, 100.0)).ok();
    }
    let limit = config.monthly_limit.as_ref().and_then(Money::number)?;
    let used = config.used.as_ref().and_then(Money::number)?;
    used.checked_div(limit)
        .and_then(|ratio| ratio.checked_mul(Decimal::ONE_HUNDRED))
}

fn window(id: String, label: Option<String>, allowance: QuotaAllowance) -> QuotaEntry {
    QuotaEntry {
        id: id.clone(),
        source_id: id,
        label,
        subject: QuotaSubject::Account,
        // Which models a credit window covers is not stated anywhere in the
        // reply, so it is not claimed.
        model_scope: QuotaScope::Unknown,
        value: QuotaValue::Window(allowance),
    }
}

pub(super) fn entries(body: &[u8]) -> Result<Vec<QuotaEntry>, ChannelError> {
    entries_with_settings(body, &Value::Null)
}

fn entries_with_settings(body: &[u8], settings: &Value) -> Result<Vec<QuotaEntry>, ChannelError> {
    let raw: Value = serde_json::from_slice(body)
        .map_err(|error| invalid_response(format!("{ID} billing: {error}")))?;
    let payload = serde_json::from_value::<BillingResponse>(raw.clone()).ok();
    let tier = settings
        .get("subscription_tier_display")
        .or_else(|| settings.get("subscription_tier"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            payload
                .as_ref()
                .and_then(|payload| payload.subscription_tier.clone())
        });
    let on_demand_enabled = settings
        .get("on_demand_enabled")
        .and_then(Value::as_bool)
        .or_else(|| {
            payload
                .as_ref()
                .and_then(|payload| payload.on_demand_enabled)
        });
    let config = payload
        .and_then(|payload| payload.config)
        .or_else(|| serde_json::from_value::<BillingConfig>(raw).ok());
    let Some(config) = config else {
        return Ok(Vec::new());
    };
    let period_end_ms = config
        .current_period
        .as_ref()
        .and_then(|period| period.end.as_deref())
        .or(config.billing_period_end.as_deref())
        .and_then(iso_to_ms);
    let period_start_ms = config
        .current_period
        .as_ref()
        .and_then(|period| period.start.as_deref())
        .or(config.billing_period_start.as_deref())
        .and_then(iso_to_ms);
    let mut entries = Vec::new();
    if config.credit_usage_percent.is_some()
        || config.current_period.is_some()
        || config.monthly_limit.is_some()
        || config.used.is_some()
        || config.billing_period_end.is_some()
    {
        let id = config
            .current_period
            .as_ref()
            .and_then(|period| period.period_type.as_deref())
            .map(period_window_key)
            .unwrap_or(USAGE_DIMENSION);
        let used = config
            .used
            .as_ref()
            .and_then(Money::number)
            .map(|cents| cents / Decimal::ONE_HUNDRED);
        let limit = config
            .monthly_limit
            .as_ref()
            .and_then(Money::number)
            .map(|cents| cents / Decimal::ONE_HUNDRED);
        entries.push(window(
            id.to_owned(),
            match (tier, config.is_unified_billing_user) {
                (Some(tier), Some(true)) => Some(format!("{tier} · Shared credits")),
                (Some(tier), _) => Some(tier),
                (None, Some(true)) => Some("Shared credits".into()),
                _ => None,
            },
            QuotaAllowance {
                used,
                limit,
                remaining: limit
                    .zip(used)
                    .map(|(limit, used)| limit.saturating_sub(used)),
                used_percent: used_percent(&config),
                unlimited: None,
                unit: (used.is_some() || limit.is_some()).then(|| "USD".into()),
                period_start_ms,
                period_end_ms,
                reset_behavior: QuotaResetBehavior::Periodic,
            },
        ));
    }
    let dollars = |money: Option<&Money>| {
        money
            .and_then(Money::number)
            .map(|cents| cents / Decimal::ONE_HUNDRED)
    };
    if config.on_demand_cap.is_some() || config.on_demand_used.is_some() {
        let limit = dollars(config.on_demand_cap.as_ref());
        let used = dollars(config.on_demand_used.as_ref());
        entries.push(window(
            "on_demand".into(),
            Some(
                match on_demand_enabled {
                    Some(false) => "On-demand credits (disabled)",
                    _ => "On-demand credits",
                }
                .into(),
            ),
            QuotaAllowance {
                used,
                limit,
                remaining: limit
                    .zip(used)
                    .map(|(limit, used)| (limit - used).max(Decimal::ZERO)),
                used_percent: used
                    .zip(limit)
                    .and_then(|(used, limit)| used.checked_div(limit))
                    .map(|ratio| ratio * Decimal::ONE_HUNDRED),
                unit: Some("USD".into()),
                period_start_ms,
                period_end_ms,
                reset_behavior: QuotaResetBehavior::Periodic,
                ..Default::default()
            },
        ));
    }
    for history in &config.history {
        let Some(cycle) = &history.billing_cycle else {
            continue;
        };
        for (kind, label, money) in [
            (
                "included",
                "Included credits",
                history.included_used.as_ref(),
            ),
            (
                "on_demand",
                "On-demand credits",
                history.on_demand_used.as_ref(),
            ),
            ("total", "Total credits", history.total_used.as_ref()),
        ] {
            if let Some(used) = dollars(money) {
                entries.push(window(
                    format!("history:{}-{:02}:{kind}", cycle.year, cycle.month),
                    Some(format!("{}-{:02} · {label}", cycle.year, cycle.month)),
                    QuotaAllowance {
                        used: Some(used),
                        unit: Some("USD".into()),
                        ..Default::default()
                    },
                ));
            }
        }
    }
    if let Some(balance) = config.prepaid_balance.as_ref().and_then(Money::number) {
        entries.push(QuotaEntry {
            id: "prepaid_balance".into(),
            source_id: "prepaid_balance".into(),
            label: Some("Prepaid credits".into()),
            subject: QuotaSubject::Account,
            model_scope: QuotaScope::Unknown,
            value: QuotaValue::Balance(QuotaBalance {
                remaining: Some(balance / Decimal::ONE_HUNDRED),
                unit: Some("USD".into()),
            }),
        });
    }
    for product in config.product_usage.iter().flatten() {
        let Some(percent) = product
            .usage_percent
            .and_then(|p| Decimal::try_from(p).ok())
        else {
            continue;
        };
        let label = product
            .product
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        entries.push(window(
            format!("product:{}", slug(label.unwrap_or("Product"))),
            label.map(str::to_owned),
            QuotaAllowance {
                used: None,
                limit: None,
                remaining: None,
                used_percent: Some(percent),
                unlimited: None,
                unit: None,
                period_start_ms,
                period_end_ms,
                reset_behavior: QuotaResetBehavior::Periodic,
            },
        ));
    }
    Ok(entries)
}

impl QuotaQuery for super::GrokBuild {
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let config = GrokBuildConfig::from_view(context.provider)?;
            let mut headers = HeaderMap::new();
            apply(
                &mut headers,
                &config,
                &context.credential,
                Reply::Json,
                None,
            )?;
            // The billing surface spells the account id without the dash the
            // data plane uses.
            if let Some(user) = super::auth::fact(&context.credential, "sub") {
                headers.insert(
                    HeaderName::from_static("x-userid"),
                    HeaderValue::from_str(user).map_err(|_| ChannelError::InvalidCredential)?,
                );
            }
            let url = format!(
                "{}/billing?format=credits",
                config.usage_base_url(context.provider)
            );
            let (status, _, body) =
                send(context.client, Method::GET, &url, headers.clone(), None).await?;
            let body = require_success(status, body)?;
            let mut entries = entries(&body)?;
            if entries.is_empty() {
                return Err(invalid_response(format!("{ID} billing: no readings")));
            }
            super::auth::apply_catalog_identity(&mut headers, &context.credential)?;
            let url = format!("{}/settings", config.usage_base_url(context.provider));
            // The CLI displays settings when available, but billing still works if settings are unavailable.
            if let Ok((status, _, settings)) =
                send(context.client, Method::GET, &url, headers, None).await
                && status.is_success()
                && let Ok(settings) = serde_json::from_slice::<Value>(&settings)
            {
                entries = entries_with_settings(&body, &settings)?;
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

    #[test]
    fn a_wrapped_payload_yields_the_period_and_each_product() {
        let body = br#"{"config":{
          "currentPeriod":{"type":"USAGE_PERIOD_TYPE_WEEKLY",
            "start":"2026-07-08T18:30:33+00:00","end":"2026-07-15T18:30:33+00:00"},
          "creditUsagePercent":2.0,
          "productUsage":[{"product":"Api","usagePercent":2.0}]
        }}"#;
        let read = entries(body).unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].id, "weekly_limit");
        let QuotaValue::Window(window) = &read[0].value else {
            panic!("a window");
        };
        assert_eq!(window.used_percent, Some(Decimal::from(2)));
        // Boundaries are upstream-exact, never derived from a window length.
        assert_eq!(window.period_start_ms, Some(1_783_535_433_000));
        assert_eq!(window.period_end_ms, Some(1_784_140_233_000));
        assert_eq!(read[1].id, "product:api");
        assert_eq!(read[1].label.as_deref(), Some("Api"));
    }

    #[test]
    fn a_bare_payload_derives_its_percentage_from_what_it_states() {
        let body = br#"{
          "monthlyLimit":{"val":2000},
          "used":{"val":"500"},
          "billingPeriodEnd":"2026-09-01T00:00:00+00:00"
        }"#;
        let read = entries(body).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].id, USAGE_DIMENSION);
        let QuotaValue::Window(window) = &read[0].value else {
            panic!("a window");
        };
        assert_eq!(window.used_percent, Some(Decimal::from(25)));
        assert_eq!(window.remaining, Some(Decimal::from(15)));
        assert_eq!(window.unit.as_deref(), Some("USD"));
        assert_eq!(window.period_start_ms, None);
        assert!(entries(b"{}").unwrap().is_empty());
    }
}
