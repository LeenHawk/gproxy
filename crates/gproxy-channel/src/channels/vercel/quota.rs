//! The team's prepaid balance. V3's /v1/credits probe is free; paid spend
//! reports are deliberately not exposed as an automatic quota probe.
use super::{Vercel, api_key, base_url};
use crate::channel::{
    ChannelError, CredentialContext, CredentialView, OperationFuture, ProviderView, QuotaBalance,
    QuotaDimension, QuotaEntry, QuotaMetric, QuotaModel, QuotaQuery, QuotaScope, QuotaSnapshot,
    QuotaSubject, QuotaTracking, QuotaValue, QuotaWindow,
};
use crate::channels::shared::compatible::ability::{bearer, decimal, require_success, send};
use http::Method;
use serde_json::Value;

pub const BALANCE_DIMENSION: &str = "vercel_balance";
impl QuotaModel for Vercel {
    fn dimensions(&self, _: ProviderView<'_>, _: CredentialView<'_>) -> Vec<QuotaDimension> {
        vec![QuotaDimension {
            id: BALANCE_DIMENSION.into(),
            label: Some("team balance".into()),
            scope: QuotaScope::All,
            operations: None,
            metric: QuotaMetric::Cost,
            window: QuotaWindow::Total,
            limit: None,
            tracking: QuotaTracking::Reported,
            blocking: true,
        }]
    }
}
impl QuotaQuery for Vercel {
    fn query<'a>(&'a self, ctx: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let base = base_url(ctx.provider);
            let base = base.strip_suffix("/v1").unwrap_or(base);
            let (status, _, body) = send(
                ctx.client,
                Method::GET,
                &format!("{base}/v1/credits"),
                bearer(api_key(ctx.credential.secret)?)?,
                None,
            )
            .await?;
            let bytes = require_success(status, body)?;
            let body: Value = serde_json::from_slice(&bytes)
                .map_err(|e| ChannelError::InvalidResponse(e.to_string()))?;
            let remaining = body.get("balance").and_then(decimal).ok_or_else(|| {
                ChannelError::InvalidResponse("Vercel credits response has no balance".into())
            })?;
            Ok(QuotaSnapshot {
                observed_at_ms: 0,
                entries: vec![QuotaEntry {
                    id: BALANCE_DIMENSION.into(),
                    source_id: BALANCE_DIMENSION.into(),
                    label: Some("team balance".into()),
                    subject: QuotaSubject::Organization,
                    model_scope: QuotaScope::All,
                    value: QuotaValue::Balance(QuotaBalance {
                        remaining: Some(remaining),
                        unit: Some("USD".into()),
                    }),
                }],
            })
        })
    }
}
