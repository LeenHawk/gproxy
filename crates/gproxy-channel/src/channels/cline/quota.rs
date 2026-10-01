//! The two account surfaces Cline answers: a credit balance and the plan's
//! usage windows.
//!
//! `GET {base}/users/{user_id}/balance` reports the account's remaining
//! credits and needs the identity the login recorded, so it is only asked
//! when a login discovered one. `GET {base}/users/me/plan/usage-limits`
//! reports `limits[]` of `{type, percentUsed, resetsAt}` — a percentage and
//! a reset, never a total — and is asked with the pasted key when the
//! credential has one, because that is the form the dashboard endpoint keys
//! on (v3 `cline/quota.rs`).
//!
//! Known plan windows apply to `cline-pass/` models. Credits apply to paid
//! models outside that namespace; zero credits never exhaust a subscription.

use super::auth;
use super::config::{ID, base_url};
use crate::channel::{
    ChannelError, CredentialContext, CredentialView, OperationFuture, ProviderView, QuotaAllowance,
    QuotaBalance, QuotaDimension, QuotaEntry, QuotaMetric, QuotaModel, QuotaQuery,
    QuotaResetBehavior, QuotaScope, QuotaSnapshot, QuotaSubject, QuotaTracking, QuotaValue,
    QuotaWindow,
};
use crate::channels::shared::compatible::ability::{decimal, send};
use crate::channels::shared::compatible::http::invalid_response;
use http::{HeaderMap, HeaderValue, Method, header};
use rust_decimal::Decimal;
use serde_json::Value;
use std::borrow::Cow;

/// The credits an account spends from.
pub const BALANCE_DIMENSION: &str = "cline_balance";
/// The source every plan window is observed under.
pub const PLAN_SOURCE: &str = "cline_plan_usage";

fn plan_dimension(kind: &str) -> Option<QuotaDimension> {
    let window = match kind {
        "five_hour" => QuotaWindow::Rolling {
            seconds: 5 * 60 * 60,
        },
        "weekly" => QuotaWindow::Rolling {
            seconds: 7 * 24 * 60 * 60,
        },
        "monthly" => QuotaWindow::CalendarMonth,
        _ => return None,
    };
    Some(QuotaDimension {
        id: format!("{PLAN_SOURCE}:{kind}"),
        label: Some(label(kind)),
        scope: QuotaScope::ModelPrefixes(vec!["cline-pass".into()]),
        operations: None,
        metric: QuotaMetric::Unit("percent".into()),
        window,
        limit: Some(Decimal::ONE_HUNDRED),
        tracking: QuotaTracking::Reported,
        blocking: true,
    })
}

impl QuotaModel for super::Cline {
    fn allows_paid_usage(&self, credential: CredentialView<'_>, dimension: &str) -> bool {
        credential
            .metadata
            .get("allow_paid_usage")
            .and_then(Value::as_bool)
            == Some(true)
            && matches!(
                dimension,
                "cline_plan_usage:five_hour"
                    | "cline_plan_usage:weekly"
                    | "cline_plan_usage:monthly"
            )
    }

    fn block_applies_to(&self, dimension: &str, model: Option<&str>) -> bool {
        let Some(model) = model else { return true };
        let free = model.starts_with("cline-free/") || model.ends_with(":free");
        if dimension == BALANCE_DIMENSION {
            return !free && !model.starts_with("cline-pass/");
        }
        if dimension
            .strip_prefix(PLAN_SOURCE)
            .is_some_and(|suffix| suffix.starts_with(':'))
        {
            return !free && model.starts_with("cline-pass/");
        }
        true
    }

    fn dimensions(
        &self,
        _: ProviderView<'_>,
        credential: CredentialView<'_>,
    ) -> Vec<QuotaDimension> {
        let mut dimensions = Vec::new();
        // A pasted key can query plan windows without a known account id.
        if auth::fact(&credential, "user_id").is_some() {
            dimensions.push(QuotaDimension {
                id: BALANCE_DIMENSION.into(),
                label: Some("account credits".into()),
                scope: QuotaScope::All,
                operations: None,
                metric: QuotaMetric::Unit("credits".into()),
                window: QuotaWindow::Total,
                limit: None,
                tracking: QuotaTracking::Reported,
                blocking: true,
            });
        }
        dimensions.extend(
            ["five_hour", "weekly", "monthly"]
                .into_iter()
                .filter_map(plan_dimension),
        );
        dimensions
    }

    fn classify<'d>(
        &self,
        declared: &'d [QuotaDimension],
        entry: &QuotaEntry,
    ) -> Option<Cow<'d, QuotaDimension>> {
        if entry.source_id == PLAN_SOURCE {
            let id = format!("{PLAN_SOURCE}:{}", entry.id);
            return declared
                .iter()
                .find(|dimension| dimension.id == id)
                .map(Cow::Borrowed);
        }
        let dimension = crate::channel::classify_by_id(declared, entry)?;
        if entry.source_id == BALANCE_DIMENSION {
            if matches!(entry.model_scope, QuotaScope::ExceptModels(_)) {
                let mut dimension = dimension.into_owned();
                dimension.scope = entry.model_scope.clone();
                return Some(Cow::Owned(dimension));
            }
            if matches!(&entry.value, QuotaValue::Balance(balance) if balance.remaining.is_some_and(|value| value <= Decimal::ZERO))
            {
                // Without a catalog, the balance cannot identify paid models.
                return None;
            }
        }
        Some(dimension)
    }
}

fn text(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn iso_to_ms(value: &str) -> Option<i64> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|stamp| stamp.unix_timestamp() * 1_000)
}

/// The envelope every Cline account endpoint wraps its payload in.
fn data(body: &[u8]) -> Result<Value, ChannelError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| invalid_response(format!("{ID} quota: {error}")))?;
    if value.get("success").and_then(Value::as_bool) != Some(true) {
        return Err(invalid_response(format!(
            "{ID} quota: the upstream did not report success"
        )));
    }
    value
        .get("data")
        .cloned()
        .ok_or_else(|| invalid_response(format!("{ID} quota: no data")))
}

/// A human label for the window types the plan is known to name; anything
/// else keeps the upstream's own word for it.
fn label(kind: &str) -> String {
    match kind {
        "five_hour" => "5-hour plan usage".into(),
        "weekly" => "weekly plan usage".into(),
        "monthly" => "monthly plan usage".into(),
        other => other.to_owned(),
    }
}

/// `limits[]` as windows. The upstream states a percentage and a reset and
/// nothing else, so no total is inferred for it.
fn plan_entries(data: &Value) -> Result<Vec<QuotaEntry>, ChannelError> {
    let limits = data
        .get("limits")
        .and_then(Value::as_array)
        .filter(|limits| !limits.is_empty())
        .ok_or_else(|| invalid_response(format!("{ID} plan usage: no limits")))?;
    let mut seen = std::collections::BTreeSet::new();
    limits
        .iter()
        .map(|limit| {
            let kind = text(limit.get("type"))
                .ok_or_else(|| invalid_response(format!("{ID} plan usage: a limit has no type")))?;
            if !seen.insert(kind) {
                return Err(invalid_response(format!(
                    "{ID} plan usage: `{kind}` reported twice"
                )));
            }
            let used_percent = limit
                .get("percentUsed")
                .filter(|value| value.is_number())
                .and_then(decimal)
                .filter(|percent| *percent >= Decimal::ZERO)
                .ok_or_else(|| {
                    invalid_response(format!("{ID} plan usage: `{kind}` has no percentage"))
                })?;
            let period_end_ms = match limit.get("resetsAt") {
                None | Some(Value::Null) => None,
                Some(value) => Some(value.as_str().and_then(iso_to_ms).ok_or_else(|| {
                    invalid_response(format!("{ID} plan usage: `{kind}` has no reset time"))
                })?),
            };
            Ok(QuotaEntry {
                id: kind.to_owned(),
                source_id: PLAN_SOURCE.into(),
                label: Some(label(kind)),
                subject: QuotaSubject::Account,
                model_scope: QuotaScope::ModelPrefixes(vec!["cline-pass".into()]),
                value: QuotaValue::Window(QuotaAllowance {
                    used_percent: Some(used_percent),
                    period_end_ms,
                    reset_behavior: QuotaResetBehavior::Periodic,
                    ..QuotaAllowance::default()
                }),
            })
        })
        .collect()
}

fn balance_entry(data: &Value) -> Result<QuotaEntry, ChannelError> {
    let remaining = data
        .get("balance")
        .and_then(decimal)
        .ok_or_else(|| invalid_response(format!("{ID} balance: no balance")))?;
    Ok(QuotaEntry {
        id: BALANCE_DIMENSION.into(),
        source_id: BALANCE_DIMENSION.into(),
        label: Some("account credits".into()),
        subject: QuotaSubject::Account,
        model_scope: QuotaScope::All,
        value: QuotaValue::Balance(QuotaBalance {
            remaining: Some(remaining),
            unit: Some("credits".into()),
        }),
    })
}

/// Percent-encode one path segment; an account id reaches the URL.
fn encode_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

async fn non_credit_models(context: &CredentialContext<'_>) -> Option<Vec<String>> {
    let mut headers = HeaderMap::new();
    auth::apply(&mut headers, &context.credential).ok()?;
    let (status, _, body) = send(
        context.client,
        Method::GET,
        &format!("{}/ai/cline/recommended-models", base_url(context.provider)),
        headers,
        None,
    )
    .await
    .ok()?;
    if !status.is_success() {
        return None;
    }
    let payload: Value = serde_json::from_slice(&body).ok()?;
    let free = payload.get("free")?.as_array()?;
    let pass = payload.get("clinePass").and_then(Value::as_array);
    free.iter()
        .chain(pass.into_iter().flatten())
        .map(|model| model.get("id")?.as_str().map(str::to_owned))
        .collect()
}

impl QuotaQuery for super::Cline {
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let base = base_url(context.provider);
            let mut entries = Vec::new();
            let mut failure = None;

            // The plan endpoint takes the pasted key verbatim when there is
            // one and the account token otherwise.
            let mut headers = HeaderMap::new();
            headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
            match auth::api_key(&context.credential) {
                Some(key) => {
                    headers.insert(
                        header::AUTHORIZATION,
                        HeaderValue::from_str(&format!("Bearer {key}"))
                            .map_err(|_| ChannelError::InvalidCredential)?,
                    );
                }
                None => auth::apply(&mut headers, &context.credential)?,
            }
            let (status, _, body) = send(
                context.client,
                Method::GET,
                &format!("{base}/users/me/plan/usage-limits"),
                headers,
                None,
            )
            .await?;
            match status.is_success().then(|| data(&body)) {
                Some(Ok(data)) => match plan_entries(&data) {
                    Ok(mut plan) => entries.append(&mut plan),
                    Err(error) => failure = Some(error),
                },
                Some(Err(error)) => failure = Some(error),
                None => failure = Some(ChannelError::UpstreamResponse { status, body }),
            }

            // The balance is a second endpoint keyed on the account id the
            // login recorded; an account without one simply has no balance
            // reading, which is not a failure of the plan reading.
            if let Some(user) = auth::fact(&context.credential, "user_id") {
                let mut headers = HeaderMap::new();
                headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
                auth::apply(&mut headers, &context.credential)?;
                let (status, _, body) = send(
                    context.client,
                    Method::GET,
                    &format!("{base}/users/{}/balance", encode_segment(user)),
                    headers,
                    None,
                )
                .await?;
                match status.is_success().then(|| data(&body)) {
                    Some(Ok(data)) => match balance_entry(&data) {
                        Ok(mut entry) => {
                            if let Some(models) = non_credit_models(&context).await {
                                entry.model_scope = QuotaScope::ExceptModels(models);
                            }
                            entries.push(entry);
                        }
                        Err(error) => failure = failure.or(Some(error)),
                    },
                    Some(Err(error)) => failure = failure.or(Some(error)),
                    None => {
                        failure = failure.or(Some(ChannelError::UpstreamResponse { status, body }));
                    }
                }
            }

            // One surface answering is a snapshot; neither answering is the
            // first refusal, reported rather than a snapshot of nothing.
            match (entries.is_empty(), failure) {
                (true, Some(error)) => Err(error),
                (_, _) => Ok(QuotaSnapshot {
                    // The host stamps receipt; neither payload carries a clock.
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
    fn a_plan_window_keeps_the_percentage_and_the_reset_without_inventing_a_total() {
        let data = json!({"limits": [
            {"type": "five_hour", "percentUsed": 0.5, "resetsAt": "2030-01-01T05:00:00Z"},
            {"type": "weekly", "percentUsed": 62, "resetsAt": null},
            {"type": "future_window", "percentUsed": 105.5},
        ]});
        let entries = plan_entries(&data).unwrap();
        assert_eq!(entries.len(), 3);
        let QuotaValue::Window(five) = &entries[0].value else {
            panic!("a window");
        };
        assert_eq!(five.used_percent, Some("0.5".parse().unwrap()));
        assert_eq!(five.period_end_ms, Some(1_893_474_000_000));
        assert_eq!((five.used, five.limit, five.remaining), (None, None, None));
        let QuotaValue::Window(weekly) = &entries[1].value else {
            panic!("a window");
        };
        assert_eq!(weekly.period_end_ms, None);
        assert_eq!(entries[2].label.as_deref(), Some("future_window"));
        assert!(entries.iter().all(|entry| entry.source_id == PLAN_SOURCE));
    }

    #[test]
    fn an_unreadable_plan_reply_is_a_failure_rather_than_an_empty_reading() {
        for limit in [
            json!({"type": "five_hour"}),
            json!({"type": "five_hour", "percentUsed": "50"}),
            json!({"type": "five_hour", "percentUsed": -1}),
            json!({"percentUsed": 50}),
            json!({"type": "five_hour", "percentUsed": 50, "resetsAt": "invalid"}),
        ] {
            assert!(plan_entries(&json!({"limits": [limit]})).is_err());
        }
        let twice = json!({"type": "five_hour", "percentUsed": 50});
        assert!(plan_entries(&json!({"limits": [twice, twice]})).is_err());
        assert!(plan_entries(&json!({"limits": []})).is_err());
        assert!(data(br#"{"success":false}"#).is_err());
    }
}
