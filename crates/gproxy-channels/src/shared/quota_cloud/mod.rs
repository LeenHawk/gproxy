mod aliyun;
mod aws;
mod fields;
mod limits;
mod prepare;
#[cfg(test)]
mod tests;

use gproxy_channel_api::{
    ChannelError, QuotaEntry, QuotaKind, QuotaQueryMode, QuotaSource, QuotaSourcePage, QuotaSupport,
};
use http::StatusCode;
use serde_json::Value;

use super::quota_api::field;
pub(crate) use fields::fields;
pub(crate) use prepare::prepare;
pub(crate) use prepare::prepare_page;

pub(crate) fn sources(channel: &str, secret: &Value, settings: &Value) -> Option<Vec<QuotaSource>> {
    let (id, label, kind, ready, reason) = match channel {
        "aws-bedrock" => (
            "management_quota",
            "AWS Bedrock applied service quotas",
            QuotaKind::RateLimit,
            aws::identity(secret).is_some(),
            "Uses IAM servicequotas:ListServiceQuotas, or GetServiceQuota when quota_quota_code is set. Reuses AWS signing credentials; Bedrock bearer keys cannot query management APIs. Lists applied limits, not consumption. Cost Explorer is never queried.",
        ),
        "vertex" | "aistudio" | "vertexexpress" => (
            "management_quota",
            "Google project service quota limits",
            QuotaKind::RateLimit,
            token(secret).is_some()
                && setting(secret, settings, "quota_project_id", "project_id").is_some(),
            "Requires cloud-platform or cloud-platform.read-only OAuth token and serviceusage.quotas.get on the configured project. Reuses an existing access_token; API keys and unexchanged service-account JWT keys need a quota_api_key access token. Lists effective limits, not remaining quota.",
        ),
        "azure" => (
            "management_quota",
            "Azure subscription regional capacity",
            QuotaKind::RateLimit,
            token(secret).is_some()
                && setting(secret, settings, "quota_subscription_id", "subscription_id").is_some()
                && setting(secret, settings, "quota_region", "region").is_some(),
            "Requires an Entra token for https://management.azure.com/ and Microsoft.CognitiveServices/locations/usages/read on the configured subscription and region. Shows allocated capacity, not token consumption or balance.",
        ),
        "dashscope" => (
            "balance",
            "Alibaba Cloud available account amount",
            QuotaKind::Balance,
            aliyun::identity(secret).is_some(),
            "Requires Alibaba Cloud AccessKey credentials with RAM bss:DescribeAcccount permission. Optional STS session token is supported. Available amount may include credit facilities; cash and credit components are shown separately.",
        ),
        "cloudflare-ai-gateway" => (
            "balance",
            "Cloudflare AI Gateway credit balance",
            QuotaKind::Balance,
            cf_token(secret).is_some()
                && setting(secret, settings, "quota_account_id", "account_id").is_some(),
            "Requires AI Gateway Read or Write on the configured account; reuses the existing Cloudflare token. The API does not declare the balance currency or scaling, so its raw value is shown without currency conversion.",
        ),
        _ => return None,
    };
    Some(vec![QuotaSource {
        id: id.into(),
        label: label.into(),
        kinds: vec![kind],
        mode: if ready {
            QuotaQueryMode::Probe
        } else {
            QuotaQueryMode::Unavailable
        },
        support: if ready {
            QuotaSupport::Ready
        } else {
            QuotaSupport::RequiresAuthorization
        },
        reason: Some(reason.into()),
        automatic: matches!(kind, QuotaKind::Balance),
    }])
}

pub(crate) fn parse(
    channel: &str,
    source: &str,
    status: StatusCode,
    body: &[u8],
) -> Result<Vec<QuotaEntry>, ChannelError> {
    let page = parse_page(channel, source, status, body)?;
    if page.next_cursor.is_some() {
        return Err(invalid(
            "Cloud quota response requires the paginated query interface",
        ));
    }
    Ok(page.entries)
}

pub(crate) fn parse_page(
    channel: &str,
    source: &str,
    status: StatusCode,
    body: &[u8],
) -> Result<QuotaSourcePage, ChannelError> {
    if !status.is_success() {
        return Err(invalid("Cloud quota endpoint returned an error"));
    }
    let raw: Value =
        serde_json::from_slice(body).map_err(|_| invalid("Invalid cloud quota JSON"))?;
    let entries = match (channel, source) {
        ("aws-bedrock", "management_quota") => aws::parse(&raw),
        ("vertex" | "aistudio" | "vertexexpress", "management_quota") => limits::google(&raw),
        ("azure", "management_quota") => limits::azure(&raw),
        ("dashscope", "balance") => aliyun::parse(&raw),
        ("cloudflare-ai-gateway", "balance") => limits::cloudflare(&raw),
        _ => Err(invalid("Unknown cloud quota source")),
    }?;
    let next_cursor = match channel {
        "aws-bedrock" => cursor(&raw, "NextToken")?,
        "vertex" | "aistudio" | "vertexexpress" => cursor(&raw, "nextPageToken")?,
        "azure" => cursor(&raw, "nextLink")?,
        _ => None,
    };
    Ok(QuotaSourcePage {
        entries,
        next_cursor,
    })
}

fn setting<'a>(
    secret: &'a Value,
    settings: &'a Value,
    name: &str,
    fallback: &str,
) -> Option<&'a str> {
    field(secret, name)
        .or_else(|| field(secret, fallback))
        .or_else(|| field(settings, fallback))
}

fn token(secret: &Value) -> Option<&str> {
    field(secret, "quota_api_key").or_else(|| field(secret, "access_token"))
}
fn cf_token(secret: &Value) -> Option<&str> {
    field(secret, "quota_api_key").or_else(|| field(secret, "api_key"))
}
fn invalid(message: &str) -> ChannelError {
    ChannelError::Prepare(message.into())
}

fn cursor(raw: &Value, field_name: &str) -> Result<Option<String>, ChannelError> {
    match raw.get(field_name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok((!value.is_empty()).then(|| value.clone())),
        Some(_) => Err(invalid("Invalid cloud quota pagination cursor")),
    }
}
