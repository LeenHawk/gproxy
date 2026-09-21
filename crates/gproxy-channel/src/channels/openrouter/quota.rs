//! The key's own budget, read from `/v1/auth/key`.
//!
//! OpenRouter scopes spending to the API key rather than the account: the
//! endpoint answers with the key's credit limit, what it has spent and the
//! request rate limit attached to it. A key with `limit: null` is bounded by
//! the account balance instead, which this probe reports as unlimited.

use super::OpenRouter;
use super::config::{DEFAULT_BASE_URL, ID};
use crate::channel::{
    ChannelError, CredentialContext, CredentialView, OperationFuture, ProviderView, QuotaAllowance,
    QuotaDimension, QuotaEntry, QuotaMetric, QuotaModel, QuotaQuery, QuotaResetBehavior,
    QuotaScope, QuotaSnapshot, QuotaSubject, QuotaTracking, QuotaValue, QuotaWindow,
};
use crate::channels::shared::compatible::ability::{bearer, decimal, require_success, send};
use crate::channels::shared::compatible::http::invalid_response;
use http::Method;
use serde_json::Value;

/// The key's credit budget.
pub const BUDGET_DIMENSION: &str = "openrouter_key_budget";
/// The request rate limit the same endpoint reports; observed, not declared,
/// because its window is only known from a reading.
const RATE_LIMIT_ID: &str = "openrouter_key_rate_limit";

impl QuotaModel for OpenRouter {
    fn dimensions(&self, _: ProviderView<'_>, _: CredentialView<'_>) -> Vec<QuotaDimension> {
        vec![QuotaDimension {
            id: BUDGET_DIMENSION.into(),
            label: Some("key credit budget".into()),
            scope: QuotaScope::All,
            operations: None,
            metric: QuotaMetric::Cost,
            window: QuotaWindow::Total,
            // The endpoint reports the ceiling; a key may have none at all.
            limit: None,
            tracking: QuotaTracking::Reported,
        }]
    }
}

fn entries(payload: &Value) -> Vec<QuotaEntry> {
    let data = payload.get("data").unwrap_or(payload);
    let mut entries = Vec::new();
    let limit = data.get("limit").and_then(decimal);
    let used = data.get("usage").and_then(decimal);
    let remaining = data
        .get("limit_remaining")
        .and_then(decimal)
        .or_else(|| match (limit, used) {
            (Some(limit), Some(used)) => Some(limit - used),
            _ => None,
        });
    if limit.is_some() || used.is_some() || remaining.is_some() {
        entries.push(QuotaEntry {
            id: BUDGET_DIMENSION.into(),
            source_id: BUDGET_DIMENSION.into(),
            label: data
                .get("label")
                .and_then(Value::as_str)
                .filter(|label| !label.is_empty())
                .map(str::to_owned),
            subject: QuotaSubject::Key,
            model_scope: QuotaScope::All,
            value: QuotaValue::Budget(QuotaAllowance {
                used,
                limit,
                remaining,
                used_percent: None,
                // A key without a ceiling spends the account balance.
                unlimited: Some(limit.is_none()),
                unit: Some("USD".into()),
                period_start_ms: None,
                period_end_ms: None,
                reset_behavior: QuotaResetBehavior::Unknown,
            }),
        });
    }
    if let Some(requests) = data.pointer("/rate_limit/requests").and_then(decimal) {
        entries.push(QuotaEntry {
            id: RATE_LIMIT_ID.into(),
            source_id: RATE_LIMIT_ID.into(),
            label: data
                .pointer("/rate_limit/interval")
                .and_then(Value::as_str)
                .map(|interval| format!("requests per {interval}")),
            subject: QuotaSubject::Key,
            model_scope: QuotaScope::All,
            value: QuotaValue::RateLimit(QuotaAllowance {
                limit: Some(requests),
                unit: Some("requests".into()),
                // The endpoint states the ceiling, never the current window.
                ..QuotaAllowance::default()
            }),
        });
    }
    entries
}

impl QuotaQuery for OpenRouter {
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let key = context
                .credential
                .secret
                .get("api_key")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|key| !key.is_empty())
                .ok_or(ChannelError::InvalidCredential)?;
            let base = context
                .provider
                .base_url
                .map(str::trim)
                .filter(|base| !base.is_empty())
                .unwrap_or(DEFAULT_BASE_URL)
                .trim_end_matches('/');
            let (status, _, body) = send(
                context.client,
                Method::GET,
                &format!("{base}/v1/auth/key"),
                bearer(key)?,
                None,
            )
            .await?;
            let body = require_success(status, body)?;
            let payload: Value = serde_json::from_slice(&body)
                .map_err(|error| invalid_response(format!("{ID} key probe: {error}")))?;
            Ok(QuotaSnapshot {
                // The host stamps receipt; the payload carries no clock.
                observed_at_ms: 0,
                entries: entries(&payload),
            })
        })
    }
}
