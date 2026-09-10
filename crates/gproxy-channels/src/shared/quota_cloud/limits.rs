use super::super::quota_balances::{amount, entry};
use super::{field, invalid};
use gproxy_channel_api::{
    ChannelError, QuotaAllowance, QuotaAvailability, QuotaBalance, QuotaEntry, QuotaScope,
    QuotaSubject, QuotaValue,
};
use rust_decimal::Decimal;
use serde_json::Value;

pub(super) fn google(raw: &Value) -> Result<Vec<QuotaEntry>, ChannelError> {
    if !raw.is_object() || raw.get("error").is_some() {
        return Err(invalid("Google service quota query failed"));
    }
    let mut entries = Vec::new();
    for metric in list(raw, "metrics")? {
        for limit in list(metric, "consumerQuotaLimits")? {
            let name =
                field(limit, "name").ok_or_else(|| invalid("Missing Google quota limit name"))?;
            for bucket in list(limit, "quotaBuckets")? {
                let dimensions = bucket
                    .get("dimensions")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                let dimensions = dimensions
                    .as_object()
                    .ok_or_else(|| invalid("Invalid Google quota dimensions"))?;
                let dimensions = dimensions
                    .iter()
                    .map(|(key, value)| {
                        value
                            .as_str()
                            .map(|value| (key.as_str(), value))
                            .ok_or_else(|| invalid("Invalid Google quota dimension"))
                    })
                    .collect::<Result<std::collections::BTreeMap<_, _>, _>>()?;
                let value = amount(bucket, "effectiveLimit")?;
                if value < Decimal::NEGATIVE_ONE {
                    return Err(invalid("Invalid Google effective quota limit"));
                }
                let unlimited = value == Decimal::NEGATIVE_ONE;
                let scope = dimensions
                    .iter()
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let mut entry = entry(
                    "management_quota",
                    &format!(
                        "{name}:{}",
                        serde_json::to_string(&dimensions).expect("string map serializes")
                    ),
                    QuotaSubject::Project,
                    QuotaValue::RateLimit(QuotaAllowance {
                        limit: (!unlimited).then_some(value),
                        unlimited,
                        unit: field(limit, "unit")
                            .or_else(|| field(metric, "unit"))
                            .map(str::to_owned),
                        ..Default::default()
                    }),
                );
                let label = field(metric, "displayName").unwrap_or(name);
                entry.label = Some(if scope.is_empty() {
                    label.into()
                } else {
                    format!("{label} ({scope})")
                });
                if let Some(model) = dimensions.get("model") {
                    entry.model_scope = QuotaScope::Models(vec![(*model).into()]);
                }
                entries.push(entry);
            }
        }
    }
    Ok(entries)
}

pub(super) fn azure(raw: &Value) -> Result<Vec<QuotaEntry>, ChannelError> {
    raw.get("value")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("Missing Azure quota usages"))?
        .iter()
        .map(|usage| {
            let id = field(&usage["name"], "value")
                .ok_or_else(|| invalid("Missing Azure quota metric name"))?;
            let limit = amount(usage, "limit")?;
            let used = amount(usage, "currentValue")?;
            let mut entry = entry(
                "management_quota",
                id,
                QuotaSubject::Unknown,
                QuotaValue::RateLimit(QuotaAllowance {
                    limit: Some(limit),
                    used: Some(used),
                    remaining: Some(
                        limit
                            .checked_sub(used)
                            .ok_or_else(|| invalid("Azure quota amount overflow"))?,
                    ),
                    unit: field(usage, "unit").map(str::to_owned),
                    ..Default::default()
                }),
            );
            entry.label = field(&usage["name"], "localizedValue").map(str::to_owned);
            Ok(entry)
        })
        .collect()
}

pub(super) fn cloudflare(raw: &Value) -> Result<Vec<QuotaEntry>, ChannelError> {
    if raw.get("success").and_then(Value::as_bool) != Some(true)
        || raw
            .get("errors")
            .and_then(Value::as_array)
            .is_some_and(|errors| !errors.is_empty())
    {
        return Err(invalid("Cloudflare credit query did not report success"));
    }
    Ok(vec![entry(
        "balance",
        "account:credits",
        QuotaSubject::Account,
        QuotaValue::Balance(QuotaBalance {
            remaining: Some(amount(&raw["result"], "balance")?),
            unit: None,
            availability: QuotaAvailability::Unknown,
            components: vec![],
        }),
    )])
}

fn list<'a>(value: &'a Value, key: &str) -> Result<&'a [Value], ChannelError> {
    match value.get(key) {
        None => Ok(&[]),
        Some(Value::Array(values)) => Ok(values),
        Some(_) => Err(invalid("Invalid Google quota list")),
    }
}
