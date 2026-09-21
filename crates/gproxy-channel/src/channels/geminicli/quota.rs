//! Per-credential quota: `POST {base}/v1internal:retrieveUserQuota` with
//! `{"project": <id>}`, the same call the model directory reads.
//!
//! One bucket per model and token type: `remainingFraction` is the share
//! LEFT in [0, 1], `resetTime` is ISO-8601, and a limit arrives as
//! `quotaAmount`, `maxAmount` or `limit` depending on the server generation,
//! as a number or a numeric string (v3 `geminicli/quota.rs`).
//!
//! There is no `QuotaModel`: which buckets an account has is only known once
//! the endpoint reports them, and nothing on the credential says. The
//! entries below are observed under their own ids. Code Assist reports no
//! rate-limit response headers either, so there is no `QuotaHeaders`.

use super::{GeminiCli, GeminiCliConfig, quota_request};
use crate::channel::{
    ChannelError, CredentialContext, OperationFuture, QuotaEntry, QuotaQuery, QuotaScope,
    QuotaSnapshot,
};
use crate::channels::shared::code_assist;
use crate::channels::shared::code_assist::quota::{component, decimal, iso_to_ms, used_percent};
use http::Method;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct UserQuota {
    #[serde(default)]
    buckets: Vec<Bucket>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Bucket {
    model_id: Option<String>,
    token_type: Option<String>,
    remaining_fraction: Option<f64>,
    remaining_amount: Option<Value>,
    #[serde(default, alias = "quotaAmount", alias = "maxAmount")]
    limit: Option<Value>,
    reset_time: Option<String>,
}

/// `<model>:<token type>`, with either half replaced by `unknown` and an
/// index standing in when the bucket names neither (v3 `window_key`).
fn window_key(model: Option<&str>, token_type: Option<&str>, index: usize) -> String {
    let model = model.map(str::trim).filter(|value| !value.is_empty());
    let token_type = token_type.map(str::trim).filter(|value| !value.is_empty());
    match (model, token_type) {
        (Some(model), Some(kind)) => format!("{}:{}", component(model), component(kind)),
        (Some(model), None) => format!("{}:unknown", component(model)),
        (None, Some(kind)) => format!("unknown:{}", component(kind)),
        (None, None) => format!("bucket_{index}"),
    }
}

impl Bucket {
    fn entry(&self, index: usize) -> QuotaEntry {
        let fraction = self.remaining_fraction.filter(|value| value.is_finite());
        let remaining = self.remaining_amount.as_ref().and_then(decimal);
        // A server generation that reports an amount but no limit still
        // implies one through the fraction it is a share of.
        let derived = remaining.zip(fraction).and_then(|(remaining, fraction)| {
            (remaining >= Decimal::ZERO && fraction > 0.0 && fraction <= 1.0)
                .then(|| remaining.checked_div(Decimal::try_from(fraction).ok()?))
                .flatten()
        });
        let limit = self
            .limit
            .as_ref()
            .and_then(decimal)
            .or(derived)
            .filter(|value| *value >= Decimal::ZERO);
        let percent = fraction.and_then(used_percent);
        let amounts = limit.and_then(|limit| {
            let spent = match (remaining, percent) {
                (Some(remaining), _) => (limit - remaining).max(Decimal::ZERO),
                (None, Some(percent)) => {
                    (limit * percent / Decimal::ONE_HUNDRED).max(Decimal::ZERO)
                }
                (None, None) => return None,
            };
            Some((spent, limit))
        });
        code_assist::quota::window(
            window_key(self.model_id.as_deref(), self.token_type.as_deref(), index),
            self.token_type.clone(),
            self.model_id
                .as_deref()
                .map(|model| QuotaScope::Models(vec![code_assist::model_id(model).to_owned()]))
                .unwrap_or_default(),
            percent,
            amounts,
            self.reset_time.as_deref().and_then(iso_to_ms),
        )
    }
}

pub(super) fn entries(body: &[u8]) -> Result<Vec<QuotaEntry>, ChannelError> {
    let payload: UserQuota = serde_json::from_slice(body).map_err(|error| {
        code_assist::invalid_response(format!("Code Assist quota response JSON: {error}"))
    })?;
    Ok(payload
        .buckets
        .iter()
        .enumerate()
        .map(|(index, bucket)| bucket.entry(index))
        .collect())
}

impl QuotaQuery for GeminiCli {
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let config = GeminiCliConfig::from_view(context.provider)?;
            let (url, headers, body) =
                quota_request(&config, context.provider, &context.credential)?;
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
