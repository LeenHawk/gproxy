use gproxy_channel_api::{
    ChannelError, QuotaAllowance, QuotaAvailability, QuotaBalance, QuotaComponent, QuotaEntry,
    QuotaSubject, QuotaValue,
};
use rust_decimal::Decimal;
use serde_json::Value;

pub(super) fn prepaid(raw: &Value) -> Result<Vec<QuotaEntry>, ChannelError> {
    // xAI records purchases as negative and spending as positive ledger amounts.
    let signed = super::super::quota_balances::amount(&raw["total"], "val")? / Decimal::from(100);
    let mut entry = super::super::quota_balances::entry(
        "prepaid_balance",
        "prepaid:USD",
        QuotaSubject::Organization,
        QuotaValue::Balance(QuotaBalance {
            remaining: Some(-signed),
            unit: Some("USD".into()),
            availability: QuotaAvailability::Unknown,
            components: vec![QuotaComponent {
                kind: "signed_credit_balance".into(),
                amount: signed,
            }],
        }),
    );
    entry.label = Some("Team prepaid balance".into());
    Ok(vec![entry])
}

pub(super) fn postpaid(raw: &Value) -> Result<Vec<QuotaEntry>, ChannelError> {
    let limit = super::super::quota_balances::amount(&raw["spendingLimits"]["effectiveSl"], "val")?
        / Decimal::from(100);
    let mut entry = super::super::quota_balances::entry(
        "postpaid_budget",
        "postpaid:monthly",
        QuotaSubject::Organization,
        QuotaValue::Budget(QuotaAllowance {
            limit: Some(limit),
            unit: Some("USD".into()),
            ..Default::default()
        }),
    );
    entry.label = Some("Effective monthly postpaid spending limit".into());
    Ok(vec![entry])
}
