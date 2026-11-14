//! The account's AI Gateway credit balance and its Workers AI neurons.
//!
//! `GET {base}/client/v4/accounts/{account}/ai-gateway/billing/credit-balance`
//! answers the usual Cloudflare envelope, `{success, errors, result:
//! {balance}}` (v3 `shared/quota_cloud`). The token needs AI Gateway Read on
//! the account; a credential whose inference token lacks it can carry a
//! separate `quota_api_key`, and `quota_account_id` when the balance lives on
//! another account, exactly the fields v3 read. The reply declares neither
//! currency nor scale, so the balance is reported as given, without a unit.
//! The credits pay for third-party models; a spent balance does not stop
//! Workers AI (`@cf/`) models, which draw on neurons.
//!
//! Workers AI gives every account 10,000 neurons a day, reset at 00:00 UTC;
//! past that a Workers Free account is refused and a Workers Paid account is
//! billed (`workers-ai/platform/pricing`). Today's consumption is the
//! `aiInferenceAdaptiveGroups` dataset of the GraphQL Analytics API,
//! `sum { totalNeurons }` filtered on the UTC date, which needs Account
//! Analytics Read. Paid usage past the free allocation follows the
//! credential's `allow_paid_usage`.
//!
//! Each reading needs its own permission, so either may fail alone; the
//! snapshot carries what was read and fails only when neither was.

use super::{CloudflareAiGateway, account_id, base_url, field};
use crate::channel::{
    ChannelError, CredentialContext, CredentialView, OperationFuture, ProviderView, QuotaAllowance,
    QuotaBalance, QuotaDimension, QuotaEntry, QuotaMetric, QuotaModel, QuotaQuery,
    QuotaResetBehavior, QuotaScope, QuotaSnapshot, QuotaSubject, QuotaTracking, QuotaValue,
    QuotaWindow,
};
use crate::channels::shared::compatible::ability::{decimal, require_success, send};
use crate::channels::shared::compatible::http::invalid_response;
use gproxy_client::OutboundClient;
use http::{HeaderMap, HeaderValue, Method, header};
use rust_decimal::Decimal;
use serde_json::{Value, json};

pub const BALANCE_DIMENSION: &str = "cloudflare_ai_gateway_balance";
pub const NEURONS_DIMENSION: &str = "cloudflare_workers_ai_neurons";
/// The free daily allocation every account has.
const FREE_NEURONS: i64 = 10_000;
const NEURONS_UNIT: &str = "neurons";
const DAY_MS: i64 = 86_400_000;
/// Workers AI model ids; `ModelPrefixes` matches them on the `/` boundary.
const WORKERS_AI_PREFIX: &str = "@cf";

const NEURONS_QUERY: &str = "query($account: String!, $date: Date!) { viewer { accounts(filter: {accountTag: $account}) { aiInferenceAdaptiveGroups(limit: 1, filter: {date: $date}) { sum { totalNeurons } } } } }";

fn workers_ai(model: &str) -> bool {
    QuotaScope::ModelPrefixes(vec![WORKERS_AI_PREFIX.into()]).matches(model)
}

impl QuotaModel for CloudflareAiGateway {
    fn dimensions(&self, _: ProviderView<'_>, _: CredentialView<'_>) -> Vec<QuotaDimension> {
        vec![
            QuotaDimension {
                id: BALANCE_DIMENSION.into(),
                label: Some("AI Gateway credits".into()),
                scope: QuotaScope::All,
                operations: None,
                metric: QuotaMetric::Cost,
                window: QuotaWindow::Total,
                limit: None,
                tracking: QuotaTracking::Reported,
                blocking: true,
            },
            QuotaDimension {
                id: NEURONS_DIMENSION.into(),
                label: Some("Workers AI free neurons".into()),
                scope: QuotaScope::ModelPrefixes(vec![WORKERS_AI_PREFIX.into()]),
                operations: None,
                metric: QuotaMetric::Unit(NEURONS_UNIT.into()),
                window: QuotaWindow::CalendarDay,
                limit: Some(FREE_NEURONS.into()),
                tracking: QuotaTracking::Reported,
                blocking: true,
            },
        ]
    }

    fn allows_paid_usage(&self, credential: CredentialView<'_>, dimension: &str) -> bool {
        dimension == NEURONS_DIMENSION
            && credential
                .metadata
                .get("allow_paid_usage")
                .and_then(Value::as_bool)
                == Some(true)
    }

    /// A spent credit balance stops third-party models only.
    fn block_applies_to(&self, dimension: &str, model: Option<&str>) -> bool {
        match dimension {
            BALANCE_DIMENSION => !model.is_some_and(workers_ai),
            NEURONS_DIMENSION => model.is_some_and(workers_ai),
            _ => true,
        }
    }
}

/// The balance out of Cloudflare's envelope; a reply that does not say it
/// succeeded is not read as a zero balance.
fn balance(body: &Value) -> Result<Decimal, ChannelError> {
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

/// Today's neurons out of a GraphQL reply. A day without inference has no
/// group at all, which is zero; a reply with errors is not.
fn neurons(body: &Value) -> Result<Decimal, ChannelError> {
    if body
        .get("errors")
        .and_then(Value::as_array)
        .is_some_and(|errors| !errors.is_empty())
    {
        let message = body
            .pointer("/errors/0/message")
            .and_then(Value::as_str)
            .unwrap_or("error");
        return Err(invalid_response(format!(
            "Cloudflare neuron query failed: {message}"
        )));
    }
    let groups = body
        .pointer("/data/viewer/accounts/0/aiInferenceAdaptiveGroups")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_response("Cloudflare neuron query has no account"))?;
    match groups.first() {
        None => Ok(Decimal::ZERO),
        Some(group) => group
            .pointer("/sum/totalNeurons")
            .and_then(decimal)
            .ok_or_else(|| invalid_response("Cloudflare neuron query has no total")),
    }
}

fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// The UTC day containing `now_ms`: its `YYYY-MM-DD` and `[start, end)`.
fn utc_day(now_ms: i64) -> Result<(String, i64, i64), ChannelError> {
    let start = now_ms - now_ms.rem_euclid(DAY_MS);
    let date = time::OffsetDateTime::from_unix_timestamp(start / 1000)
        .map_err(|error| invalid_response(error.to_string()))?
        .date();
    Ok((date.to_string(), start, start + DAY_MS))
}

fn bearer(token: &str) -> Result<HeaderMap, ChannelError> {
    let mut headers = HeaderMap::new();
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| ChannelError::InvalidCredential)?,
    );
    Ok(headers)
}

async fn balance_entry(
    client: &dyn OutboundClient,
    base: &str,
    account: &str,
    token: &str,
) -> Result<QuotaEntry, ChannelError> {
    let (status, _, body) = send(
        client,
        Method::GET,
        &format!("{base}/client/v4/accounts/{account}/ai-gateway/billing/credit-balance"),
        bearer(token)?,
        None,
    )
    .await?;
    let body = require_success(status, body)?;
    let payload: Value =
        serde_json::from_slice(&body).map_err(|error| invalid_response(error.to_string()))?;
    Ok(QuotaEntry {
        id: BALANCE_DIMENSION.into(),
        source_id: BALANCE_DIMENSION.into(),
        label: Some("AI Gateway credits".into()),
        subject: QuotaSubject::Account,
        model_scope: QuotaScope::All,
        value: QuotaValue::Balance(QuotaBalance {
            remaining: Some(balance(&payload)?),
            unit: None,
        }),
    })
}

async fn neurons_entry(
    client: &dyn OutboundClient,
    base: &str,
    account: &str,
    token: &str,
) -> Result<QuotaEntry, ChannelError> {
    let (date, start, end) = utc_day(now_ms())?;
    let mut headers = bearer(token)?;
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    let request = json!({
        "query": NEURONS_QUERY,
        "variables": {"account": account, "date": date},
    });
    let (status, _, body) = send(
        client,
        Method::POST,
        &format!("{base}/client/v4/graphql"),
        headers,
        Some(serde_json::to_vec(&request).map_err(|error| invalid_response(error.to_string()))?),
    )
    .await?;
    let body = require_success(status, body)?;
    let payload: Value =
        serde_json::from_slice(&body).map_err(|error| invalid_response(error.to_string()))?;
    let used = neurons(&payload)?;
    let limit = Decimal::from(FREE_NEURONS);
    Ok(QuotaEntry {
        id: NEURONS_DIMENSION.into(),
        source_id: NEURONS_DIMENSION.into(),
        label: Some("Workers AI free neurons".into()),
        subject: QuotaSubject::Account,
        model_scope: QuotaScope::ModelPrefixes(vec![WORKERS_AI_PREFIX.into()]),
        value: QuotaValue::Window(QuotaAllowance {
            used: Some(used),
            limit: Some(limit),
            remaining: Some((limit - used).max(Decimal::ZERO)),
            unit: Some(NEURONS_UNIT.into()),
            period_start_ms: Some(start),
            period_end_ms: Some(end),
            reset_behavior: QuotaResetBehavior::Periodic,
            ..QuotaAllowance::default()
        }),
    })
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
            let base = base_url(ctx.provider);
            let readings = [
                balance_entry(ctx.client, base, &account, token).await,
                neurons_entry(ctx.client, base, &account, token).await,
            ];
            let mut entries = Vec::new();
            let mut failure = None;
            for reading in readings {
                match reading {
                    Ok(entry) => entries.push(entry),
                    Err(error) => {
                        failure.get_or_insert(error);
                    }
                }
            }
            match (entries.is_empty(), failure) {
                (true, Some(error)) => Err(error),
                _ => Ok(QuotaSnapshot {
                    observed_at_ms: 0,
                    entries,
                }),
            }
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
