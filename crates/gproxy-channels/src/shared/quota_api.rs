use bytes::Bytes;
use gproxy_channel_api::{ChannelError, QuotaEntry};
use http::{Request, StatusCode};
use serde_json::Value;

pub(crate) fn prepare(
    channel: &str,
    source: &str,
    secret: &Value,
    settings: &Value,
) -> Result<Option<Request<Bytes>>, ChannelError> {
    if let Some(request) = super::quota_cloud::prepare(channel, source, secret, settings)? {
        return Ok(Some(request));
    }
    if let Some(request) = super::quota_management::prepare(channel, source, secret, settings)? {
        return Ok(Some(request));
    }
    let (default, path) = match (channel, source) {
        ("deepseek", "balance") => ("https://api.deepseek.com", "/user/balance"),
        ("openrouter", "key") => ("https://openrouter.ai/api", "/v1/key"),
        ("vercel", "balance") => ("https://ai-gateway.vercel.sh", "/v1/credits"),
        ("claudeapi", "organization_usage") => {
            ("https://api.anthropic.com", "/v1/organizations/cost_report")
        }
        ("kimi", "balance") => ("https://api.moonshot.cn", "/v1/users/me/balance"),
        ("custom", "siliconflow_balance") if super::quota_catalog::siliconflow(settings) => {
            ("https://api.siliconflow.cn", "/v1/user/info")
        }
        _ => return Ok(None),
    };
    let base = field(settings, "base_url")
        .or_else(|| {
            (channel == "kimi")
                .then(|| field(secret, "base_url"))
                .flatten()
        })
        .unwrap_or(default);
    let key =
        field(secret, "api_key").ok_or_else(|| ChannelError::Secret("api_key missing".into()))?;
    let query = match (channel, source) {
        ("claudeapi", "organization_usage") => Some(super::quota_claude_report::query()),
        _ => None,
    };
    let uri = super::http::exact(base, None)?;
    let base_path = uri.path().trim_end_matches('/');
    let base_path = base_path.strip_suffix("/v1").unwrap_or(base_path);
    let query = super::http::merge_query(uri.query(), query.as_deref());
    let path = format!("{base_path}{path}");
    let path_and_query = query.map_or_else(|| path.clone(), |query| format!("{path}?{query}"));
    let mut parts = uri.into_parts();
    parts.path_and_query = Some(
        path_and_query
            .parse()
            .map_err(|_| ChannelError::Prepare("Invalid quota endpoint path".into()))?,
    );
    let uri = http::Uri::from_parts(parts)
        .map_err(|_| ChannelError::Prepare("Invalid quota endpoint URL".into()))?;
    let uri = super::http::strip_userinfo(uri)?;
    let request = if channel == "claudeapi" {
        Request::get(uri)
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
    } else {
        Request::get(uri).header("authorization", format!("Bearer {key}"))
    };
    request
        .header("accept", "application/json")
        .body(Bytes::new())
        .map(Some)
        .map_err(|_| {
            ChannelError::Prepare("Invalid quota request URL or authorization header".into())
        })
}

pub(crate) fn parse(
    channel: &str,
    source: &str,
    status: StatusCode,
    body: &[u8],
) -> Result<Vec<QuotaEntry>, ChannelError> {
    if !status.is_success() {
        return Err(ChannelError::Prepare(format!(
            "Upstream quota query returned HTTP {status}"
        )));
    }
    if cloud(channel) {
        return super::quota_cloud::parse(channel, source, status, body);
    }
    if management(channel, source) {
        return super::quota_management::parse(channel, source, status, body);
    }
    let raw: Value = serde_json::from_slice(body)
        .map_err(|_| ChannelError::Prepare("Invalid upstream quota JSON".into()))?;
    match (channel, source) {
        ("deepseek", "balance") => super::quota_balances::deepseek(&raw),
        ("kimi", "balance") => super::quota_balances::moonshot(&raw),
        ("vercel", "balance") => super::quota_balances::vercel(&raw),
        ("claudeapi", "organization_usage") => super::quota_claude_report::parse(&raw),
        ("openrouter", "key") => super::quota_balances::openrouter(&raw),
        ("custom", "siliconflow_balance") => super::quota_balances::siliconflow(&raw),
        _ => Err(ChannelError::Prepare(
            "Unknown upstream quota source".into(),
        )),
    }
}

pub(crate) fn field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

pub(crate) fn cloud(channel: &str) -> bool {
    matches!(
        channel,
        "aws-bedrock"
            | "vertex"
            | "aistudio"
            | "vertexexpress"
            | "azure"
            | "dashscope"
            | "cloudflare-ai-gateway"
    )
}
pub(crate) fn management(channel: &str, source: &str) -> bool {
    matches!(
        (channel, source),
        ("openrouter", "account_balance")
            | ("openai" | "claudeapi", "organization_usage")
            | ("xai", "prepaid_balance" | "postpaid_budget")
            | ("opencode", "go_subscription" | "console_balance")
    )
}

pub(crate) fn prepare_page(
    channel: &str,
    source: &str,
    secret: &Value,
    settings: &Value,
    cursor: Option<&str>,
) -> Result<Option<Request<Bytes>>, ChannelError> {
    if cloud(channel) {
        return super::quota_cloud::prepare_page(channel, source, secret, settings, cursor);
    }
    if management(channel, source) {
        return super::quota_management::prepare_page(channel, source, secret, settings, cursor);
    }
    if cursor.is_some() {
        return Err(ChannelError::Prepare(
            "Unexpected quota pagination cursor".into(),
        ));
    }
    prepare(channel, source, secret, settings)
}

pub(crate) fn parse_page(
    channel: &str,
    source: &str,
    status: StatusCode,
    body: &[u8],
) -> Result<gproxy_channel_api::QuotaSourcePage, ChannelError> {
    if cloud(channel) {
        return super::quota_cloud::parse_page(channel, source, status, body);
    }
    if management(channel, source) {
        return super::quota_management::parse_page(channel, source, status, body);
    }
    Ok(gproxy_channel_api::QuotaSourcePage {
        entries: parse(channel, source, status, body)?,
        next_cursor: None,
    })
}
