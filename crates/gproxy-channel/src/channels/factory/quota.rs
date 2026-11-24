//! Usage windows and the extra-usage balance from `GET /api/billing/limits`.
//!
//! droid 1.18.0 (`x/js/0626.js`, the token-limit service) reads
//! `{hasActiveSubscription, limits: {standard, core}, extraUsageBalanceCents,
//! overagePreference, extraUsageAllowed, canManageOverage}`. Each pool holds
//! `fiveHour`, `weekly` and `monthly`, each `{usedPercent, windowEnd}` with
//! `windowEnd` an ISO time or null; a pool is exhausted when any window
//! reaches 100. `standard` covers every model but the Droid Core ones, which
//! draw on `core` (`billingPool` in the registry, `x/js/0073.js`).

use super::config::{FactoryConfig, ID};
use super::models;
use super::request::{auth_headers, identity_headers};
use crate::channel::{
    QuotaAllowance, QuotaBalance, QuotaEntry, QuotaResetBehavior, QuotaScope, QuotaSnapshot,
    QuotaSubject, QuotaValue,
};
use crate::channels::shared::compatible::ability::{decimal, require_success, send};
use crate::channels::shared::compatible::http::invalid_response;
use gproxy_upstream::{CredentialContext, UpstreamError, UpstreamFuture};
use http::{HeaderMap, HeaderValue, Method, header};
use rust_decimal::Decimal;
use serde_json::Value;

/// The source every Factory reading is observed under.
pub const QUOTA_SOURCE: &str = "factory_billing";

const POOLS: [(&str, &str); 2] = [("standard", "Standard"), ("core", "Droid Core")];
const WINDOWS: [(&str, &str, &str); 3] = [
    ("fiveHour", "five_hour", "5-hour"),
    ("weekly", "weekly", "weekly"),
    ("monthly", "monthly", "monthly"),
];

fn iso_to_ms(value: &str) -> Option<i64> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|stamp| {
            i64::try_from(stamp.unix_timestamp_nanos() / 1_000_000).unwrap_or(i64::MAX)
        })
}

fn entries(payload: &Value) -> Result<Vec<QuotaEntry>, UpstreamError> {
    let core = models::core_ids();
    let mut out = Vec::new();
    let limits = payload.get("limits");
    for (pool, pool_label) in POOLS {
        let Some(windows) = limits.and_then(|limits| limits.get(pool)).filter(|v| v.is_object())
        else {
            continue;
        };
        let scope = if pool == "core" {
            QuotaScope::Models(core.clone())
        } else {
            QuotaScope::ExceptModels(core.clone())
        };
        for (key, id, label) in WINDOWS {
            let Some(window) = windows.get(key).filter(|v| v.is_object()) else {
                continue;
            };
            let used_percent = window
                .get("usedPercent")
                .and_then(decimal)
                .ok_or_else(|| {
                    invalid_response(format!("{ID} billing limits: `{pool}.{key}` has no usedPercent"))
                })?;
            out.push(QuotaEntry {
                id: format!("{pool}_{id}"),
                source_id: QUOTA_SOURCE.into(),
                label: Some(format!("{pool_label} {label} usage")),
                subject: QuotaSubject::Account,
                model_scope: scope.clone(),
                value: QuotaValue::Window(QuotaAllowance {
                    used_percent: Some(used_percent.clamp(Decimal::ZERO, Decimal::from(100))),
                    period_end_ms: window
                        .get("windowEnd")
                        .and_then(Value::as_str)
                        .and_then(iso_to_ms),
                    reset_behavior: QuotaResetBehavior::Periodic,
                    ..QuotaAllowance::default()
                }),
            });
        }
    }
    if let Some(cents) = payload.get("extraUsageBalanceCents").and_then(decimal) {
        out.push(QuotaEntry {
            id: "extra_usage".into(),
            source_id: QUOTA_SOURCE.into(),
            label: Some("Extra usage balance".into()),
            subject: QuotaSubject::Account,
            model_scope: QuotaScope::All,
            value: QuotaValue::Balance(QuotaBalance {
                remaining: Some(cents / Decimal::from(100)),
                unit: Some("USD".into()),
            }),
        });
    }
    if out.is_empty() && payload.get("hasActiveSubscription") != Some(&Value::Bool(false)) {
        return Err(invalid_response(format!("{ID} billing limits: no usage windows")));
    }
    Ok(out)
}

impl crate::channel::QuotaQuery for super::Factory {
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> UpstreamFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let config = FactoryConfig::from_view(context.provider)?;
            let mut headers = HeaderMap::new();
            auth_headers(&context.credential, &mut headers)?;
            identity_headers(&config, &mut headers)?;
            // Control-plane calls go out with Bun's own agent, not the CLI's.
            headers.insert(header::USER_AGENT, HeaderValue::from_static("Bun/1.4.5"));
            headers.insert(header::ACCEPT, HeaderValue::from_static("*/*"));
            let (status, _, body) = send(
                context.client,
                Method::GET,
                &format!("{}/api/billing/limits", FactoryConfig::base_url(context.provider)),
                headers,
                None,
            )
            .await?;
            let body = require_success(status, body)?;
            let payload: Value = serde_json::from_slice(&body)
                .map_err(|error| invalid_response(format!("{ID} billing limits: {error}")))?;
            Ok(QuotaSnapshot {
                observed_at_ms: 0,
                entries: entries(&payload)?,
            })
        })
    }
}
