//! The team's prepaid balance and postpaid ceiling, from the management API.
//!
//! Neither lives on the inference host and neither answers to the inference
//! key: both need an xAI *management* key and a team id, which the provider
//! configures. Amounts arrive as signed integer cents, purchases recorded
//! negative, so remaining credit is the negated hundredth of `total.val`.

use super::Xai;
use super::config::{ID, XaiConfig};
use crate::channel::{
    ChannelError, CredentialContext, CredentialView, OperationFuture, ProviderView, QuotaAllowance,
    QuotaBalance, QuotaDimension, QuotaEntry, QuotaMetric, QuotaModel, QuotaQuery,
    QuotaResetBehavior, QuotaScope, QuotaSnapshot, QuotaSubject, QuotaTracking, QuotaValue,
    QuotaWindow,
};
use crate::channels::shared::compatible::ability::{bearer, decimal, require_success, send};
use crate::channels::shared::compatible::http::invalid_response;
use http::Method;
use rust_decimal::Decimal;
use serde_json::Value;

/// Credit bought up front.
pub const PREPAID_DIMENSION: &str = "xai_prepaid_balance";
/// The monthly ceiling on postpaid spending.
pub const POSTPAID_DIMENSION: &str = "xai_postpaid_budget";

const CENTS: Decimal = Decimal::from_parts(100, 0, 0, false, 0);

impl QuotaModel for Xai {
    /// Nothing is reported without a management key and a team, so an
    /// unconfigured provider declares no dimension rather than one that can
    /// never be filled.
    fn dimensions(&self, provider: ProviderView<'_>, _: CredentialView<'_>) -> Vec<QuotaDimension> {
        let Ok(config) = XaiConfig::from_view(provider) else {
            return Vec::new();
        };
        if config.billing().is_none() {
            return Vec::new();
        }
        vec![
            QuotaDimension {
                id: PREPAID_DIMENSION.into(),
                label: Some("prepaid credit".into()),
                scope: QuotaScope::All,
                operations: None,
                metric: QuotaMetric::Cost,
                window: QuotaWindow::Total,
                limit: None,
                tracking: QuotaTracking::Reported,
            },
            QuotaDimension {
                id: POSTPAID_DIMENSION.into(),
                label: Some("postpaid monthly limit".into()),
                scope: QuotaScope::All,
                operations: None,
                metric: QuotaMetric::Cost,
                window: QuotaWindow::CalendarMonth,
                limit: None,
                tracking: QuotaTracking::Reported,
            },
        ]
    }
}

fn prepaid(payload: &Value) -> Option<QuotaEntry> {
    let cents = payload.pointer("/total/val").and_then(decimal)?;
    Some(QuotaEntry {
        id: PREPAID_DIMENSION.into(),
        source_id: PREPAID_DIMENSION.into(),
        label: Some("prepaid credit".into()),
        subject: QuotaSubject::Organization,
        model_scope: QuotaScope::All,
        value: QuotaValue::Balance(QuotaBalance {
            // Purchases are recorded as negative cents.
            remaining: Some(-cents / CENTS),
            unit: Some("USD".into()),
        }),
    })
}

fn postpaid(payload: &Value) -> Option<QuotaEntry> {
    let cents = payload
        .pointer("/spendingLimits/effectiveSl/val")
        .and_then(decimal)?;
    Some(QuotaEntry {
        id: POSTPAID_DIMENSION.into(),
        source_id: POSTPAID_DIMENSION.into(),
        label: Some("postpaid monthly limit".into()),
        subject: QuotaSubject::Organization,
        model_scope: QuotaScope::All,
        value: QuotaValue::Budget(QuotaAllowance {
            limit: Some(cents / CENTS),
            unit: Some("USD".into()),
            reset_behavior: QuotaResetBehavior::Periodic,
            ..QuotaAllowance::default()
        }),
    })
}

impl QuotaQuery for Xai {
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let config = XaiConfig::from_view(context.provider)?;
            let (key, team) = config.billing().ok_or_else(|| {
                ChannelError::InvalidConfig(format!(
                    "{ID} billing needs quota_api_key and quota_team_id"
                ))
            })?;
            let base = config.management_url();
            let mut entries = Vec::new();
            for (path, parse) in [
                (
                    format!("{base}/v1/billing/teams/{team}/prepaid/balance"),
                    prepaid as fn(&Value) -> Option<QuotaEntry>,
                ),
                (
                    format!("{base}/v1/billing/teams/{team}/postpaid/spending-limits"),
                    postpaid,
                ),
            ] {
                let (status, _, body) =
                    send(context.client, Method::GET, &path, bearer(key)?, None).await?;
                let body = require_success(status, body)?;
                let payload: Value = serde_json::from_slice(&body)
                    .map_err(|error| invalid_response(format!("{ID} billing: {error}")))?;
                entries.extend(parse(&payload));
            }
            Ok(QuotaSnapshot {
                observed_at_ms: 0,
                entries,
            })
        })
    }
}
