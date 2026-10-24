//! AI settings calls DescribeUserUsageStats with feature=hunyuan_ai
//! (ot_setting.js, Kt -> useUsageStats; features.js:94967). Its fields are
//! quotaUsed, quotaLimit and quotaStartDate; -1 is the explicit unlimited limit.
//! The member page identifies this meter as AI questions, not tokens or money.
//! Public DescribeProductEditions (captured 2026-10-24) has monthly and daily
//! plans, so neither a month boundary nor the account's limit is hard-coded.
//!
//! This account meter does not identify which LightAI model routes it
//! covers. It is an observe-only reading: no QuotaModel blocks every model or
//! counts requests locally against an unverified pool. Reset instants are
//! exposed only when the AI stats themselves return an unambiguous timestamp.

use super::{
    OrcaTerm, auth,
    config::{OrcaTermConfig, invalid, now_ms},
};
use crate::channel::{
    ChannelError, CredentialContext, OperationFuture, QuotaAllowance, QuotaEntry, QuotaQuery,
    QuotaResetBehavior, QuotaScope, QuotaSnapshot, QuotaSubject, QuotaValue,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};

const FEATURE: &str = "hunyuan_ai";

fn amount(stats: &Value, field: &str) -> Result<Option<Decimal>, ChannelError> {
    let text = match stats.get(field) {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::String(value)) => value.trim().to_owned(),
        _ => return Err(invalid(format!("invalid AI quota {field}"))),
    };
    text.parse()
        .map(Some)
        .map_err(|_| invalid(format!("invalid AI quota {field}")))
}

fn timestamp(value: &Value) -> Option<i64> {
    // No timezone is implied for a date-only/naive string, and a number's
    // seconds-versus-milliseconds unit has not been established for this API.
    time::OffsetDateTime::parse(
        value.as_str()?,
        &time::format_description::well_known::Rfc3339,
    )
    .ok()
    .and_then(|date| i64::try_from(date.unix_timestamp_nanos() / 1_000_000).ok())
}

fn entry(stats: &Value) -> Result<QuotaEntry, ChannelError> {
    if stats
        .get("feature")
        .is_some_and(|feature| feature != FEATURE)
    {
        return Err(invalid("usage reply is not the requested AI feature"));
    }
    let used = amount(stats, "quotaUsed")?;
    let raw_limit = amount(stats, "quotaLimit")?;
    if used.is_none() && raw_limit.is_none() {
        return Err(invalid(
            "AI quota reply reported neither quotaUsed nor quotaLimit",
        ));
    }
    if used.is_some_and(|value| value < Decimal::ZERO)
        || raw_limit.is_some_and(|value| value < Decimal::ZERO && value != -Decimal::ONE)
    {
        return Err(invalid("AI quota reply has an invalid negative counter"));
    }
    let unlimited = raw_limit.map(|value| value == -Decimal::ONE);
    let limit = raw_limit.filter(|value| *value >= Decimal::ZERO);
    let remaining = limit
        .zip(used)
        .map(|(limit, used)| limit.saturating_sub(used).max(Decimal::ZERO));
    let used_percent = used
        .zip(limit)
        .and_then(|(used, limit)| used.checked_div(limit))
        .and_then(|ratio| ratio.checked_mul(Decimal::ONE_HUNDRED));
    Ok(QuotaEntry {
        id: FEATURE.into(),
        source_id: FEATURE.into(),
        label: Some("OrcaTerm AI questions".into()),
        subject: QuotaSubject::Account,
        model_scope: QuotaScope::Unknown,
        value: QuotaValue::Window(QuotaAllowance {
            used,
            limit,
            remaining,
            used_percent,
            unlimited,
            unit: Some("requests".into()),
            period_start_ms: timestamp(&stats["quotaStartDate"]),
            period_end_ms: timestamp(&stats["quotaEndDate"]),
            reset_behavior: QuotaResetBehavior::Periodic,
        }),
    })
}

impl QuotaQuery for OrcaTerm {
    fn query<'a>(&'a self, ctx: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let config = OrcaTermConfig::from_view(ctx.provider)?;
            let mut secret = ctx.credential.secret.clone();
            // Older imports can carry the same verified identity in metadata.
            for key in ["uin", "owner_uin"] {
                if let Some(value) = auth::fact(ctx.credential, key) {
                    secret["provider_fields"][key] = json!(value);
                }
            }
            let stats = auth::rpc(
                ctx.client,
                &config,
                &secret,
                "DescribeUserUsageStats",
                json!({"feature":FEATURE}),
            )
            .await?;
            Ok(QuotaSnapshot {
                observed_at_ms: now_ms(),
                entries: vec![entry(&stats)?],
            })
        })
    }
}
