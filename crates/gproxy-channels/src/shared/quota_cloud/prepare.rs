use super::{aliyun, aws, cf_token, field, invalid, setting, token};
use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use http::{Request, Uri};
use serde_json::Value;

pub(crate) fn prepare(
    channel: &str,
    source: &str,
    secret: &Value,
    settings: &Value,
) -> Result<Option<Request<Bytes>>, ChannelError> {
    prepare_page(channel, source, secret, settings, None)
}

pub(crate) fn prepare_page(
    channel: &str,
    source: &str,
    secret: &Value,
    settings: &Value,
    cursor: Option<&str>,
) -> Result<Option<Request<Bytes>>, ChannelError> {
    let (base, path, authorization) = match (channel, source) {
        ("aws-bedrock", "management_quota") => {
            return aws::prepare(secret, settings, cursor).map(Some);
        }
        ("dashscope", "balance") if cursor.is_none() => return aliyun::prepare(secret).map(Some),
        ("vertex" | "aistudio" | "vertexexpress", "management_quota") => {
            let project = required(
                setting(secret, settings, "quota_project_id", "project_id"),
                "Google project is required",
            )?;
            let service = if channel == "aistudio" {
                "generativelanguage.googleapis.com"
            } else {
                "aiplatform.googleapis.com"
            };
            (
                "https://serviceusage.googleapis.com",
                format!(
                    "/v1beta1/projects/{}/services/{service}/consumerQuotaMetrics?view=FULL&pageSize=200",
                    segment(project)?
                ),
                token(secret),
            )
        }
        ("azure", "management_quota") => {
            let subscription = required(
                setting(secret, settings, "quota_subscription_id", "subscription_id"),
                "Azure subscription is required",
            )?;
            let region = required(
                setting(secret, settings, "quota_region", "region"),
                "Azure region is required",
            )?;
            (
                "https://management.azure.com",
                format!(
                    "/subscriptions/{}/providers/Microsoft.CognitiveServices/locations/{}/usages?api-version=2024-10-01",
                    segment(subscription)?,
                    segment(region)?
                ),
                token(secret),
            )
        }
        ("cloudflare-ai-gateway", "balance") => {
            let account = required(
                setting(secret, settings, "quota_account_id", "account_id"),
                "Cloudflare account is required",
            )?;
            (
                "https://api.cloudflare.com/client/v4",
                format!(
                    "/accounts/{}/ai-gateway/billing/credit-balance",
                    segment(account)?
                ),
                cf_token(secret),
            )
        }
        _ => return Ok(None),
    };
    let mut uri = endpoint(secret, base, &path)?;
    if let Some(cursor) = cursor {
        if channel == "azure" {
            let next = cursor
                .parse::<Uri>()
                .map_err(|_| invalid("Invalid Azure next page URL"))?;
            if next.scheme() != uri.scheme()
                || next.authority() != uri.authority()
                || next.path() != uri.path()
            {
                return Err(invalid(
                    "Azure next page URL changed the authorized quota resource",
                ));
            }
            uri = next;
        } else if matches!(channel, "vertex" | "aistudio" | "vertexexpress") {
            let query = form_urlencoded::Serializer::new(String::new())
                .append_pair("pageToken", cursor)
                .finish();
            uri = super::super::http::exact(&uri.to_string(), Some(&query))?;
        } else {
            return Err(invalid("This cloud quota source does not paginate"));
        }
    }
    Request::get(uri)
        .header(
            "authorization",
            format!(
                "Bearer {}",
                required(authorization, "Cloud management access token is required")?
            ),
        )
        .header("accept", "application/json")
        .body(Bytes::new())
        .map(Some)
        .map_err(|_| invalid("Invalid cloud management request"))
}

pub(super) fn endpoint(secret: &Value, default: &str, path: &str) -> Result<Uri, ChannelError> {
    let base = field(secret, "quota_base_url").unwrap_or(default);
    let uri = super::super::http::exact(base, None)?;
    if uri.query().is_some()
        || uri
            .authority()
            .is_some_and(|value| value.as_str().contains('@'))
    {
        return Err(invalid(
            "Cloud quota base URL must not contain user information or query parameters",
        ));
    }
    super::super::http::join(base, path, None)
}

pub(super) fn segment(value: &str) -> Result<&str, ChannelError> {
    if matches!(value, "" | "." | "..")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(invalid("Invalid cloud quota resource identifier"));
    }
    Ok(value)
}

fn required<'a>(value: Option<&'a str>, message: &str) -> Result<&'a str, ChannelError> {
    value.ok_or_else(|| ChannelError::Secret(message.into()))
}
