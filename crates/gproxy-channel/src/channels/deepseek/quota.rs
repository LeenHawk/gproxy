//! The account's remaining credit, read from `/user/balance`.
//!
//! The reply is `{"is_available": bool, "balance_infos": [{"currency",
//! "total_balance", "granted_balance", "topped_up_balance"}]}`. An account
//! may hold more than one currency, so the first becomes the declared
//! dimension's reading and the rest are reported under their own ids.

use super::DeepSeek;
use super::config::{DEFAULT_BASE_URL, ID};
use crate::channel::{
    ChannelError, CredentialContext, CredentialView, OperationFuture, ProviderView, QuotaBalance,
    QuotaDimension, QuotaEntry, QuotaMetric, QuotaModel, QuotaQuery, QuotaScope, QuotaSnapshot,
    QuotaSubject, QuotaTracking, QuotaValue, QuotaWindow,
};
use crate::channels::shared::compatible::ability::{bearer, decimal, require_success, send};
use crate::channels::shared::compatible::http::invalid_response;
use http::Method;
use serde_json::Value;

/// The prepaid balance the account spends from.
pub const BALANCE_DIMENSION: &str = "deepseek_balance";

impl QuotaModel for DeepSeek {
    fn dimensions(&self, _: ProviderView<'_>, _: CredentialView<'_>) -> Vec<QuotaDimension> {
        vec![QuotaDimension {
            id: BALANCE_DIMENSION.into(),
            label: Some("account balance".into()),
            scope: QuotaScope::All,
            operations: None,
            metric: QuotaMetric::Cost,
            window: QuotaWindow::Total,
            limit: None,
            tracking: QuotaTracking::Reported,
        }]
    }
}

fn entries(payload: &Value) -> Vec<QuotaEntry> {
    let mut entries = Vec::new();
    let balances = payload
        .get("balance_infos")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    for (index, balance) in balances.iter().enumerate() {
        let currency = balance
            .get("currency")
            .and_then(Value::as_str)
            .map(str::to_owned);
        // The first balance answers the declared dimension; further
        // currencies are extra observations under ids of their own.
        let id = match (index, &currency) {
            (0, _) => BALANCE_DIMENSION.to_owned(),
            (_, Some(currency)) => {
                format!("{BALANCE_DIMENSION}_{}", currency.to_ascii_lowercase())
            }
            (index, None) => format!("{BALANCE_DIMENSION}_{index}"),
        };
        entries.push(QuotaEntry {
            id: id.clone(),
            source_id: id,
            label: currency.clone(),
            subject: QuotaSubject::Account,
            model_scope: QuotaScope::All,
            value: QuotaValue::Balance(QuotaBalance {
                remaining: balance.get("total_balance").and_then(decimal),
                unit: currency,
            }),
        });
    }
    entries
}

impl QuotaQuery for DeepSeek {
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
                &format!("{base}/user/balance"),
                bearer(key)?,
                None,
            )
            .await?;
            let body = require_success(status, body)?;
            let payload: Value = serde_json::from_slice(&body)
                .map_err(|error| invalid_response(format!("{ID} balance: {error}")))?;
            Ok(QuotaSnapshot {
                observed_at_ms: 0,
                entries: entries(&payload),
            })
        })
    }
}
