mod fields;
mod openai;
mod opencode;
mod opencode_console;
mod requests;
mod sources;
mod xai;

pub(crate) use fields::fields;
pub(crate) use requests::{prepare, prepare_page};
pub(crate) use sources::sources;

use gproxy_channel_api::{
    ChannelError, QuotaAvailability, QuotaBalance, QuotaEntry, QuotaSourcePage, QuotaSubject,
    QuotaValue,
};
use http::StatusCode;
use serde_json::Value;

pub(crate) fn parse(
    channel: &str,
    source: &str,
    status: StatusCode,
    body: &[u8],
) -> Result<Vec<QuotaEntry>, ChannelError> {
    if channel == "opencode" && source == "console_balance" {
        return opencode_console::parse(status, body);
    }
    let raw = decode(status, body)?;
    match (channel, source) {
        ("openrouter", "account_balance") => {
            let data = &raw["data"];
            let purchased = super::quota_balances::amount(data, "total_credits")?;
            let spent = super::quota_balances::amount(data, "total_usage")?;
            let remaining = purchased.checked_sub(spent).ok_or_else(|| {
                ChannelError::Prepare("OpenRouter account balance overflow".into())
            })?;
            Ok(vec![super::quota_balances::entry(
                "account_balance",
                "balance:USD",
                QuotaSubject::Account,
                QuotaValue::Balance(QuotaBalance {
                    remaining: Some(remaining),
                    unit: Some("USD".into()),
                    availability: QuotaAvailability::Unknown,
                    components: vec![],
                }),
            )])
        }
        ("openai", "organization_usage") => openai::parse(&raw),
        ("claudeapi", "organization_usage") => super::quota_claude_report::parse(&raw),
        ("xai", "prepaid_balance") => xai::prepaid(&raw),
        ("xai", "postpaid_budget") => xai::postpaid(&raw),
        ("opencode", "go_subscription") => opencode::parse(&raw),
        _ => Err(ChannelError::Prepare(
            "Unknown management quota source".into(),
        )),
    }
}

pub(crate) fn parse_page(
    channel: &str,
    source: &str,
    status: StatusCode,
    body: &[u8],
) -> Result<QuotaSourcePage, ChannelError> {
    if channel == "openai" && source == "organization_usage" {
        return openai::parse_page(&decode(status, body)?);
    }
    Ok(QuotaSourcePage {
        entries: parse(channel, source, status, body)?,
        next_cursor: None,
    })
}

fn decode(status: StatusCode, body: &[u8]) -> Result<Value, ChannelError> {
    if !status.is_success() {
        return Err(ChannelError::Prepare(format!(
            "Upstream quota query returned HTTP {status}"
        )));
    }
    serde_json::from_slice(body)
        .map_err(|_| ChannelError::Prepare("Invalid upstream management quota JSON".into()))
}

#[cfg(test)]
mod tests;
