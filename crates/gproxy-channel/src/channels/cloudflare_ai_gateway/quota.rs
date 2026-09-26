//! The account's AI Gateway credit balance.
//!
//! `GET {base}/client/v4/accounts/{account}/ai-gateway/billing/credit-balance`
//! answers the usual Cloudflare envelope, `{success, errors, result:
//! {balance}}` (v3 `shared/quota_cloud`). The token needs AI Gateway Read on
//! the account; a credential whose inference token lacks it can carry a
//! separate `quota_api_key`, and `quota_account_id` when the balance lives on
//! another account, exactly the fields v3 read.
//!
//! The reply declares neither currency nor scale, so the balance is reported
//! as given, without a unit.

use super::{CloudflareAiGateway, account_id, base_url, field};
use crate::channel::{
    ChannelError, CredentialContext, CredentialView, OperationFuture, ProviderView, QuotaBalance,
    QuotaDimension, QuotaEntry, QuotaMetric, QuotaModel, QuotaQuery, QuotaScope, QuotaSnapshot,
    QuotaSubject, QuotaTracking, QuotaValue, QuotaWindow,
};
use crate::channels::shared::compatible::ability::{decimal, require_success, send};
use crate::channels::shared::compatible::http::invalid_response;
use http::{HeaderMap, HeaderValue, Method, header};
use serde_json::Value;

pub const BALANCE_DIMENSION: &str = "cloudflare_ai_gateway_balance";

impl QuotaModel for CloudflareAiGateway {
    fn dimensions(&self, _: ProviderView<'_>, _: CredentialView<'_>) -> Vec<QuotaDimension> {
        vec![QuotaDimension {
            id: BALANCE_DIMENSION.into(),
            label: Some("AI Gateway credits".into()),
            scope: QuotaScope::All,
            operations: None,
            metric: QuotaMetric::Cost,
            window: QuotaWindow::Total,
            limit: None,
            tracking: QuotaTracking::Reported,
        }]
    }
}

/// The balance out of Cloudflare's envelope; a reply that does not say it
/// succeeded is not read as a zero balance.
fn balance(body: &Value) -> Result<rust_decimal::Decimal, ChannelError> {
    let succeeded = body.get("success").and_then(Value::as_bool) == Some(true)
        && body
            .get("errors")
            .and_then(Value::as_array)
            .is_none_or(|errors| errors.is_empty());
    if !succeeded {
        return Err(invalid_response(
            "Cloudflare credit query did not report success",
        ));
    }
    body.pointer("/result/balance")
        .and_then(decimal)
        .ok_or_else(|| invalid_response("Cloudflare credit query has no balance"))
}

impl QuotaQuery for CloudflareAiGateway {
    fn query<'a>(&'a self, ctx: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let secret = ctx.credential.secret;
            let token = field(secret, "quota_api_key")
                .or_else(|| field(secret, "api_key"))
                .ok_or(ChannelError::InvalidCredential)?;
            let account = match field(secret, "quota_account_id") {
                Some(_) => account_id(secret, "quota_account_id")?,
                None => account_id(secret, "account_id")?,
            };
            let mut headers = HeaderMap::new();
            headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
            headers.insert(
                header::AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {token}"))
                    .map_err(|_| ChannelError::InvalidCredential)?,
            );
            let (status, _, body) = send(
                ctx.client,
                Method::GET,
                &format!(
                    "{}/client/v4/accounts/{account}/ai-gateway/billing/credit-balance",
                    base_url(ctx.provider)
                ),
                headers,
                None,
            )
            .await?;
            let body = require_success(status, body)?;
            let payload: Value = serde_json::from_slice(&body)
                .map_err(|error| invalid_response(error.to_string()))?;
            Ok(QuotaSnapshot {
                observed_at_ms: 0,
                entries: vec![QuotaEntry {
                    id: BALANCE_DIMENSION.into(),
                    source_id: BALANCE_DIMENSION.into(),
                    label: Some("AI Gateway credits".into()),
                    subject: QuotaSubject::Account,
                    model_scope: QuotaScope::All,
                    value: QuotaValue::Balance(QuotaBalance {
                        remaining: Some(balance(&payload)?),
                        unit: None,
                    }),
                }],
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_a_successful_envelope_yields_a_balance() {
        assert_eq!(
            balance(&json!({"success": true, "errors": [], "result": {"balance": "12.5"}}))
                .unwrap(),
            "12.5".parse().unwrap()
        );
        assert_eq!(
            balance(&json!({"success": true, "result": {"balance": 3}})).unwrap(),
            3.into()
        );
        for failed in [
            json!({"success": false, "result": {"balance": 1}}),
            json!({"success": true, "errors": [{"code": 10000}], "result": {"balance": 1}}),
            json!({"success": true, "result": {}}),
        ] {
            assert!(balance(&failed).is_err(), "{failed}");
        }
    }
}
