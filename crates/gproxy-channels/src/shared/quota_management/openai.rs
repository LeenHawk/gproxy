use gproxy_channel_api::{
    ChannelError, QuotaEntry, QuotaSourcePage, QuotaSubject, QuotaUsageReport, QuotaValue,
};
use rust_decimal::Decimal;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

pub(super) fn query() -> String {
    let now: i64 = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_secs()
        .try_into()
        .expect("epoch seconds fit i64");
    let end = now - now.rem_euclid(86400);
    format!(
        "start_time={}&end_time={end}&bucket_width=1d&limit=7",
        end - 7 * 86400
    )
}

pub(super) fn parse(raw: &Value) -> Result<Vec<QuotaEntry>, ChannelError> {
    let page = parse_page(raw)?;
    if page.next_cursor.is_some() {
        return Err(ChannelError::Prepare(
            "OpenAI cost report requires pagination; no partial spending total was saved".into(),
        ));
    }
    Ok(page.entries)
}

pub(super) fn parse_page(raw: &Value) -> Result<QuotaSourcePage, ChannelError> {
    let has_more = raw
        .get("has_more")
        .and_then(Value::as_bool)
        .ok_or_else(|| ChannelError::Prepare("Missing OpenAI report pagination status".into()))?;
    let next_cursor = if has_more {
        Some(
            super::super::quota_api::field(raw, "next_page")
                .ok_or_else(|| {
                    ChannelError::Prepare("Missing OpenAI report next_page cursor".into())
                })?
                .to_owned(),
        )
    } else {
        None
    };
    Ok(QuotaSourcePage {
        entries: buckets(raw)?,
        next_cursor,
    })
}

fn buckets(raw: &Value) -> Result<Vec<QuotaEntry>, ChannelError> {
    let buckets = raw
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| ChannelError::Prepare("Missing OpenAI cost report data".into()))?;
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    for bucket in buckets {
        let boundary = |name| {
            bucket
                .get(name)
                .and_then(Value::as_i64)
                .ok_or_else(|| ChannelError::Prepare(format!("Invalid OpenAI cost report {name}")))
        };
        let start = boundary("start_time")?;
        let end = boundary("end_time")?;
        if start % 86400 != 0 || end.checked_sub(start) != Some(86400) || !seen.insert(start) {
            return Err(ChannelError::Prepare(
                "Invalid or duplicate OpenAI daily cost interval".into(),
            ));
        }
        let date = time::OffsetDateTime::from_unix_timestamp(start)
            .map_err(|_| ChannelError::Prepare("Invalid OpenAI report date".into()))?
            .date();
        let rows = bucket
            .get("results")
            .and_then(Value::as_array)
            .ok_or_else(|| ChannelError::Prepare("Missing OpenAI cost report results".into()))?;
        let mut amounts = BTreeMap::<String, Decimal>::new();
        for row in rows {
            let amount = &row["amount"];
            let currency = super::super::quota_api::field(amount, "currency")
                .filter(|value| {
                    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_alphabetic())
                })
                .ok_or_else(|| ChannelError::Prepare("Invalid OpenAI report currency".into()))?
                .to_ascii_uppercase();
            let value = super::super::quota_balances::amount(amount, "value")?;
            let sum = amounts.entry(currency).or_default();
            *sum = sum.checked_add(value).ok_or_else(|| {
                ChannelError::Prepare("OpenAI cost report amount overflow".into())
            })?;
        }
        for (currency, used) in amounts {
            let mut entry = super::super::quota_balances::entry(
                "organization_usage",
                &format!("usage:{start}:{currency}"),
                QuotaSubject::Organization,
                QuotaValue::UsageReport(QuotaUsageReport {
                    used,
                    unit: currency,
                    period_start: start,
                    period_end: end,
                }),
            );
            entry.label = Some(format!("{date} UTC"));
            entries.push(entry);
        }
    }
    Ok(entries)
}
