use super::super::quota_api::field;
use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use http::Request;
use serde_json::Value;

pub(crate) fn prepare(
    channel: &str,
    source: &str,
    secret: &Value,
    settings: &Value,
) -> Result<Option<Request<Bytes>>, ChannelError> {
    if channel == "opencode" && source == "console_balance" {
        return super::opencode_console::prepare(secret, settings).map(Some);
    }
    let (default, path, query) = match (channel, source) {
        ("openrouter", "account_balance") => {
            ("https://openrouter.ai/api", "/v1/credits".into(), None)
        }
        ("openai", "organization_usage") => (
            "https://api.openai.com",
            "/v1/organization/costs".into(),
            Some(super::openai::query()),
        ),
        ("claudeapi", "organization_usage") => (
            "https://api.anthropic.com",
            "/v1/organizations/cost_report".into(),
            Some(super::super::quota_claude_report::query()),
        ),
        ("xai", "prepaid_balance" | "postpaid_budget") => {
            let team = field(secret, "quota_team_id")
                .or_else(|| field(settings, "quota_team_id"))
                .ok_or_else(|| ChannelError::Secret("quota_team_id missing".into()))?;
            let endpoint = if source == "prepaid_balance" {
                "prepaid/balance"
            } else {
                "postpaid/spending-limits"
            };
            (
                "https://management-api.x.ai",
                format!(
                    "/v1/billing/teams/{}/{endpoint}",
                    super::super::http::encode_component(team)
                ),
                None,
            )
        }
        ("opencode", "go_subscription") if super::opencode::is_go(settings) => {
            let base = field(settings, "base_url").unwrap_or("https://opencode.ai/zen/go/v1");
            let key = field(secret, "api_key")
                .or_else(|| field(secret, "access_token"))
                .ok_or_else(|| ChannelError::Secret("api_key missing".into()))?;
            return get(base, "/usage", None, key, false, false).map(Some);
        }
        _ => return Ok(None),
    };
    let management_key = field(secret, "quota_api_key");
    let key = management_key
        .or_else(|| {
            (channel == "claudeapi")
                .then(|| field(secret, "api_key"))
                .flatten()
        })
        .ok_or_else(|| ChannelError::Secret("quota_api_key missing".into()))?;
    let base = field(secret, "quota_base_url")
        .or_else(|| field(settings, "quota_base_url"))
        .or_else(|| {
            (channel == "claudeapi" && management_key.is_none())
                .then(|| field(settings, "base_url"))
                .flatten()
        })
        .unwrap_or(default);
    get(
        base,
        &path,
        query.as_deref(),
        key,
        channel == "claudeapi",
        true,
    )
    .map(Some)
}

pub(crate) fn prepare_page(
    channel: &str,
    source: &str,
    secret: &Value,
    settings: &Value,
    cursor: Option<&str>,
) -> Result<Option<Request<Bytes>>, ChannelError> {
    let Some(cursor) = cursor else {
        return prepare(channel, source, secret, settings);
    };
    if channel != "openai" || source != "organization_usage" || cursor.is_empty() {
        return Err(ChannelError::Prepare(
            "Unexpected management quota pagination cursor".into(),
        ));
    }
    let mut request = prepare(channel, source, secret, settings)?
        .ok_or_else(|| ChannelError::Prepare("Unknown paginated quota source".into()))?;
    let query = format!("page={}", super::super::http::encode_component(cursor));
    *request.uri_mut() = super::super::http::exact(&request.uri().to_string(), Some(&query))?;
    Ok(Some(request))
}

fn get(
    base: &str,
    path: &str,
    query: Option<&str>,
    key: &str,
    claude: bool,
    strip_version: bool,
) -> Result<Request<Bytes>, ChannelError> {
    let uri = super::super::http::exact(base, None)?;
    let base_path = uri.path().trim_end_matches('/');
    let base_path = if strip_version {
        base_path.strip_suffix("/v1").unwrap_or(base_path)
    } else {
        base_path
    };
    let query = super::super::http::merge_query(uri.query(), query);
    let path = format!("{base_path}{path}");
    let path = query.map_or_else(|| path.clone(), |query| format!("{path}?{query}"));
    let mut parts = uri.into_parts();
    parts.path_and_query = Some(
        path.parse()
            .map_err(|_| ChannelError::Prepare("Invalid management quota path".into()))?,
    );
    let uri = http::Uri::from_parts(parts)
        .map_err(|_| ChannelError::Prepare("Invalid management quota URL".into()))?;
    let uri = super::super::http::strip_userinfo(uri)?;
    let builder = if claude {
        Request::get(uri)
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
    } else {
        Request::get(uri).header("authorization", format!("Bearer {key}"))
    };
    builder
        .header("accept", "application/json")
        .body(Bytes::new())
        .map_err(|_| ChannelError::Prepare("Invalid management quota request headers".into()))
}
