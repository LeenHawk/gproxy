//! Per-credential quota, riding the catalogue call:
//! `POST {base}/v1internal:fetchAvailableModels` with an empty JSON body.
//! Each entry under `models` may carry a `quotaInfo` with
//! `remainingFraction` (the share LEFT in [0, 1]) and an ISO-8601
//! `resetTime` (v3 `antigravity/quota.rs`).
//!
//! There is no `QuotaModel`: which models an account has buckets for is only
//! known once the endpoint reports them, and nothing on the credential says.
//! The entries below are observed under their own ids. Code Assist reports
//! no rate-limit response headers either, so there is no `QuotaHeaders`.

use super::{Antigravity, AntigravityConfig, catalog_request, models};
use crate::channel::{
    ChannelError, CredentialContext, OperationFuture, QuotaEntry, QuotaQuery, QuotaScope,
    QuotaSnapshot,
};
use crate::channels::shared::code_assist;
use crate::channels::shared::code_assist::quota::{iso_to_ms, used_percent};
use http::Method;
use serde_json::Value;

pub(super) fn entries(body: &[u8]) -> Result<Vec<QuotaEntry>, ChannelError> {
    Ok(models::quota_info(body)?
        .iter()
        .filter_map(|(model_id, model)| {
            let quota = model.get("quotaInfo").filter(|value| value.is_object())?;
            let id = code_assist::model_id(model_id).to_owned();
            Some(code_assist::quota::window(
                id.clone(),
                None,
                QuotaScope::Models(vec![id]),
                quota
                    .get("remainingFraction")
                    .and_then(Value::as_f64)
                    .and_then(used_percent),
                None,
                code_assist::text(quota, "resetTime").and_then(iso_to_ms),
            ))
        })
        .collect())
}

impl QuotaQuery for Antigravity {
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let config = AntigravityConfig::from_view(context.provider)?;
            let (url, headers, body) =
                catalog_request(&config, context.provider, &context.credential)?;
            let (status, _, bytes) = code_assist::send(
                context.client,
                Method::POST,
                &url,
                headers,
                Some(body.to_vec()),
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                });
            }
            Ok(QuotaSnapshot {
                // The host stamps receipt; the payload carries no time.
                observed_at_ms: 0,
                entries: entries(&bytes)?,
            })
        })
    }
}
