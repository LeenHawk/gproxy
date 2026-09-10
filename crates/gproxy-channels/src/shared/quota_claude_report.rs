use gproxy_channel_api::{ChannelError, QuotaEntry, QuotaSubject, QuotaUsageReport, QuotaValue};
use rust_decimal::Decimal;
use serde_json::Value;
use time::Duration;

pub(crate) fn query() -> String {
    let now = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_secs()
        .try_into()
        .expect("epoch seconds fit i64");
    let today = time::OffsetDateTime::from_unix_timestamp(now)
        .expect("valid Unix time")
        .date();
    format!(
        "starting_at={}T00:00:00Z&ending_at={today}T00:00:00Z&bucket_width=1d&limit=7",
        today - Duration::days(7)
    )
}

pub(crate) fn parse(raw: &Value) -> Result<Vec<QuotaEntry>, ChannelError> {
    if raw.get("has_more").and_then(Value::as_bool) != Some(false) {
        return Err(ChannelError::Prepare("Claude cost report is incomplete or missing pagination status; no partial spending total was saved".into()));
    }
    let buckets = raw
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| ChannelError::Prepare("Missing Claude cost report data".into()))?;
    let mut seen = std::collections::HashSet::new();
    buckets
        .iter()
        .map(|bucket| {
            let boundary = |field| {
                super::quota_api::field(bucket, field)
                    .and_then(super::quota::iso_to_unix)
                    .ok_or_else(|| {
                        ChannelError::Prepare(format!("Invalid Claude cost report {field}"))
                    })
            };
            let start = boundary("starting_at")?;
            let end = boundary("ending_at")?;
            if start % 86400 != 0 || end.checked_sub(start) != Some(86400) || !seen.insert(start) {
                return Err(ChannelError::Prepare(
                    "Invalid or duplicate Claude daily cost report interval".into(),
                ));
            }
            let rows = bucket
                .get("results")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    ChannelError::Prepare("Missing Claude cost report results".into())
                })?;
            let mut cents = Decimal::ZERO;
            for row in rows {
                if super::quota_api::field(row, "currency") != Some("USD") {
                    return Err(ChannelError::Prepare(
                        "Unsupported Claude cost report currency".into(),
                    ));
                }
                cents = cents
                    .checked_add(super::quota_balances::amount(row, "amount")?)
                    .ok_or_else(|| {
                        ChannelError::Prepare("Claude cost report amount overflow".into())
                    })?;
            }
            let mut entry = super::quota_balances::entry(
                "organization_usage",
                &format!("usage:{start}"),
                QuotaSubject::Organization,
                QuotaValue::UsageReport(QuotaUsageReport {
                    used: cents / Decimal::from(100),
                    unit: "USD".into(),
                    period_start: start,
                    period_end: end,
                }),
            );
            entry.label = Some(format!(
                "{} UTC",
                time::OffsetDateTime::from_unix_timestamp(start)
                    .expect("parsed timestamp is valid")
                    .date()
            ));
            Ok(entry)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gproxy_channel_api::Channel;
    use serde_json::json;

    #[test]
    fn cost_report_converts_fractional_cents_and_keeps_explicit_empty_day() {
        let raw = json!({"has_more":false,"data":[
            {"starting_at":"2026-09-01T00:00:00Z","ending_at":"2026-09-02T00:00:00Z","results":[
                {"amount":"123.78912","currency":"USD"},{"amount":"1.25","currency":"USD"}]},
            {"starting_at":"2026-09-02T00:00:00Z","ending_at":"2026-09-03T00:00:00Z","results":[]}
        ]});
        let entries = parse(&raw).unwrap();
        let QuotaValue::UsageReport(report) = &entries[0].value else {
            panic!()
        };
        assert_eq!(report.used.to_string(), "1.2503912");
        let QuotaValue::UsageReport(report) = &entries[1].value else {
            panic!()
        };
        assert_eq!(report.used, Decimal::ZERO);
        assert_eq!(report.period_end - report.period_start, 86400);
        let mut partial = raw.clone();
        partial["has_more"] = json!(true);
        assert!(parse(&partial).is_err());
        partial["has_more"] = json!(false);
        partial["data"][0]["results"][0]["currency"] = json!("CNY");
        assert!(parse(&partial).is_err());
        assert!(parse(&json!({"data":[]})).is_err());
        assert!(
            parse(&json!({"has_more":false,"data":[]}))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn report_reuses_current_key_without_automatic_admin_polling() {
        let secret = json!({"api_key":"test-token"});
        let settings = json!({"base_url":"https://proxy.example/anthropic/v1"});
        let source = crate::ClaudeApiChannel
            .quota_sources(&secret, &settings)
            .into_iter()
            .find(|source| source.id == "organization_usage")
            .unwrap();
        assert!(!source.automatic);
        let request = crate::ClaudeApiChannel
            .prepare_quota_source("organization_usage", &secret, &settings)
            .unwrap()
            .unwrap();
        assert_eq!(request.headers()["x-api-key"], "test-token");
        assert_eq!(request.headers()["anthropic-version"], "2023-06-01");
        assert!(!request.headers().contains_key("authorization"));
        assert_eq!(
            request.uri().path(),
            "/anthropic/v1/organizations/cost_report"
        );
        let query = request.uri().query().unwrap();
        assert!(query.contains("bucket_width=1d&limit=7"));
        assert!(!query.contains("group_by"));
    }
}
