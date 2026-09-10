use gproxy_channel_api::{
    ChannelError, QuotaAllowance, QuotaAvailability, QuotaBalance, QuotaComponent, QuotaEntry,
    QuotaScope, QuotaSubject, QuotaValue,
};
use rust_decimal::Decimal;
use serde_json::Value;

pub(crate) fn amount(value: &Value, field: &str) -> Result<Decimal, ChannelError> {
    value
        .get(field)
        .and_then(super::quota::decimal)
        .ok_or_else(|| ChannelError::Prepare(format!("Invalid quota response field: {field}")))
}

fn optional_amount(value: &Value, field: &str) -> Result<Option<Decimal>, ChannelError> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => amount(value, field).map(Some),
    }
}

fn components(value: &Value, fields: &[(&str, &str)]) -> Result<Vec<QuotaComponent>, ChannelError> {
    fields
        .iter()
        .filter(|(field, _)| value.get(field).is_some())
        .map(|(field, kind)| {
            Ok(QuotaComponent {
                kind: (*kind).into(),
                amount: amount(value, field)?,
            })
        })
        .collect()
}

pub(crate) fn entry(
    source: &str,
    id: &str,
    subject: QuotaSubject,
    value: QuotaValue,
) -> QuotaEntry {
    QuotaEntry {
        id: id.into(),
        source_id: source.into(),
        label: None,
        subject,
        model_scope: QuotaScope::All,
        observed_at_ms: 0,
        value,
    }
}

pub(crate) fn deepseek(raw: &Value) -> Result<Vec<QuotaEntry>, ChannelError> {
    let available = raw
        .get("is_available")
        .and_then(Value::as_bool)
        .ok_or_else(|| ChannelError::Prepare("Missing DeepSeek is_available".into()))?;
    let balances = raw
        .get("balance_infos")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty())
        .ok_or_else(|| ChannelError::Prepare("Missing DeepSeek balance_infos".into()))?;
    let mut seen = std::collections::HashSet::new();
    balances
        .iter()
        .map(|value| {
            let currency = super::quota_api::field(value, "currency")
                .ok_or_else(|| ChannelError::Prepare("Missing balance currency".into()))?;
            if !seen.insert(currency) {
                return Err(ChannelError::Prepare("Duplicate balance currency".into()));
            }
            Ok(entry(
                "balance",
                &format!("balance:{currency}"),
                QuotaSubject::Account,
                QuotaValue::Balance(QuotaBalance {
                    remaining: Some(amount(value, "total_balance")?),
                    unit: Some(currency.into()),
                    availability: if available {
                        QuotaAvailability::Available
                    } else {
                        QuotaAvailability::Unavailable
                    },
                    components: components(
                        value,
                        &[
                            ("granted_balance", "granted"),
                            ("topped_up_balance", "topped_up"),
                        ],
                    )?,
                }),
            ))
        })
        .collect()
}

pub(crate) fn moonshot(raw: &Value) -> Result<Vec<QuotaEntry>, ChannelError> {
    if raw.get("code").and_then(Value::as_i64) != Some(0)
        || raw.get("status").and_then(Value::as_bool) != Some(true)
    {
        return Err(ChannelError::Prepare(
            "Moonshot balance query did not report success".into(),
        ));
    }
    let value = &raw["data"];
    let balance = amount(value, "available_balance")?;
    Ok(vec![entry(
        "balance",
        "balance:CNY",
        QuotaSubject::Account,
        QuotaValue::Balance(QuotaBalance {
            remaining: Some(balance),
            unit: Some("CNY".into()),
            // The upstream contract explicitly defines availability at this boundary.
            availability: if balance > Decimal::ZERO {
                QuotaAvailability::Available
            } else {
                QuotaAvailability::Unavailable
            },
            components: components(
                value,
                &[("voucher_balance", "voucher"), ("cash_balance", "cash")],
            )?,
        }),
    )])
}

pub(crate) fn vercel(raw: &Value) -> Result<Vec<QuotaEntry>, ChannelError> {
    Ok(vec![entry(
        "balance",
        "balance:USD",
        QuotaSubject::Organization,
        QuotaValue::Balance(QuotaBalance {
            remaining: Some(amount(raw, "balance")?),
            unit: Some("USD".into()),
            availability: QuotaAvailability::Unknown,
            components: vec![],
        }),
    )])
}

pub(crate) fn siliconflow(raw: &Value) -> Result<Vec<QuotaEntry>, ChannelError> {
    if raw.get("code").and_then(Value::as_i64) != Some(20000) {
        return Err(ChannelError::Prepare(
            "SiliconFlow user query did not report success".into(),
        ));
    }
    let value = &raw["data"];
    Ok(vec![entry(
        "siliconflow_balance",
        "balance",
        QuotaSubject::Account,
        QuotaValue::Balance(QuotaBalance {
            remaining: Some(amount(value, "totalBalance")?),
            unit: None,
            availability: QuotaAvailability::Unknown,
            components: components(
                value,
                &[("balance", "balance"), ("chargeBalance", "charge_balance")],
            )?,
        }),
    )])
}

pub(crate) fn openrouter(raw: &Value) -> Result<Vec<QuotaEntry>, ChannelError> {
    let data = &raw["data"];
    if data.get("limit").is_none() || data.get("limit_remaining").is_none() {
        return Err(ChannelError::Prepare(
            "Missing OpenRouter key budget fields".into(),
        ));
    }
    let limit = optional_amount(data, "limit")?;
    let remaining = optional_amount(data, "limit_remaining")?;
    let reset = super::quota_api::field(data, "limit_reset");
    // Upstream remaining already incorporates reset period and BYOK inclusion policy.
    let used = match (limit, remaining) {
        (Some(limit), Some(remaining)) => Some(
            limit
                .checked_sub(remaining)
                .ok_or_else(|| ChannelError::Prepare("OpenRouter budget amount overflow".into()))?,
        ),
        _ => None,
    };
    let mut budget = entry(
        "key",
        "key:budget",
        QuotaSubject::Key,
        QuotaValue::Budget(QuotaAllowance {
            limit,
            remaining,
            used,
            unlimited: data["limit"].is_null(),
            unit: Some("USD".into()),
            ..Default::default()
        }),
    );
    budget.label = Some(
        match reset {
            Some("daily") => "Daily key budget",
            Some("weekly") => "Weekly key budget",
            Some("monthly") => "Monthly key budget",
            None => "Key budget",
            Some(_) => "Key budget (provider reset schedule)",
        }
        .into(),
    );
    Ok(vec![budget])
}
