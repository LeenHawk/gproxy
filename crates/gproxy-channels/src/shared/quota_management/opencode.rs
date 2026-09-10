use gproxy_channel_api::{ChannelError, QuotaAllowance, QuotaEntry, QuotaSubject, QuotaValue};
use rust_decimal::Decimal;
use serde_json::Value;

pub(super) fn is_go(settings: &Value) -> bool {
    super::super::quota_api::field(settings, "tier") == Some("go")
        || super::super::quota_api::field(settings, "base_url")
            .and_then(|base| base.parse::<http::Uri>().ok())
            .is_some_and(|uri| {
                uri.host() == Some("opencode.ai")
                    && uri.path().trim_end_matches('/') == "/zen/go/v1"
            })
}

pub(super) fn parse(raw: &Value) -> Result<Vec<QuotaEntry>, ChannelError> {
    let usage = &raw["usage"];
    [
        ("rolling", "Rolling"),
        ("weekly", "Weekly"),
        ("monthly", "Monthly"),
    ]
    .into_iter()
    .map(|(key, label)| {
        let value = &usage[key];
        if !matches!(
            super::super::quota_api::field(value, "status"),
            Some("ok" | "rate-limited")
        ) {
            return Err(ChannelError::Prepare(
                "Invalid OpenCode Go quota status".into(),
            ));
        }
        let percent = super::super::quota_balances::amount(value, "percent")?;
        if !(Decimal::ZERO..=Decimal::from(100)).contains(&percent) {
            return Err(ChannelError::Prepare(
                "Invalid OpenCode Go quota percentage".into(),
            ));
        }
        let end = super::super::quota_api::field(value, "resetsAt")
            .and_then(super::super::quota::iso_to_unix)
            .ok_or_else(|| ChannelError::Prepare("Invalid OpenCode Go quota reset time".into()))?;
        let mut entry = super::super::quota_balances::entry(
            "go_subscription",
            key,
            QuotaSubject::Unknown,
            QuotaValue::Window(QuotaAllowance {
                used_percent: Some(percent),
                period_end: Some(end),
                ..Default::default()
            }),
        );
        entry.label = Some(format!("{label} Go quota"));
        Ok(entry)
    })
    .collect()
}
