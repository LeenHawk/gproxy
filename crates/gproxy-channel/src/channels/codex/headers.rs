//! Account facts and CLI-compatible backend headers.

use super::common::invalid_config;
use super::{CodexConfig, DEFAULT_BASE_URL, agent};
use crate::channel::{
    ChannelError, ChannelHeaders, CredentialView, HeaderAllowlist, ProviderView, forwardable,
};
use http::{HeaderMap, HeaderName, HeaderValue, header};
use serde_json::Value;

fn header_value(value: &str) -> Result<HeaderValue, ChannelError> {
    HeaderValue::from_str(value).map_err(|_| ChannelError::InvalidCredential)
}

/// The Codex API origin (`.../backend-api/codex`) and the ChatGPT backend it
/// hangs off (`.../backend-api`), which serves account endpoints.
pub(super) fn base_urls(provider: ProviderView<'_>) -> (String, String) {
    let base = provider
        .base_url
        .unwrap_or(DEFAULT_BASE_URL)
        .trim_end_matches('/')
        .to_owned();
    let backend = base
        .strip_suffix("/codex")
        .map(str::to_owned)
        .unwrap_or_else(|| base.clone());
    (base, backend)
}

/// Facts about the account from the secret and the host-persisted metadata.
pub(super) struct Account<'a> {
    pub(super) access_token: &'a str,
    pub(super) account_id: Option<String>,
}

pub(super) fn account<'a>(credential: &CredentialView<'a>) -> Result<Account<'a>, ChannelError> {
    let access_token = credential
        .secret
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .ok_or(ChannelError::InvalidCredential)?;
    let account_id = credential
        .metadata
        .get("chatgpt_account_id")
        .or_else(|| {
            credential
                .secret
                .pointer("/provider_fields/chatgpt_account_id")
        })
        .and_then(Value::as_str)
        .map(str::to_owned);
    Ok(Account {
        access_token,
        account_id,
    })
}

pub(super) fn plan_type(credential: &CredentialView<'_>) -> Option<String> {
    credential
        .metadata
        .get("plan_type")
        .or_else(|| {
            credential
                .secret
                .pointer("/provider_fields/chatgpt_plan_type")
        })
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// What the Codex CLI itself sends alongside a Responses call (session and
/// thread ids, turn metadata and sticky routing state, subagent role,
/// request id, CLI version, attestation). A provider allow-list never strips
/// these; it only narrows what other clients may add.
pub const CLI_HEADERS: ChannelHeaders = ChannelHeaders {
    names: &[
        "accept",
        "mcp-session-id",
        "mcp-protocol-version",
        "session-id",
        "thread-id",
        "version",
        "x-client-request-id",
        "x-openai-subagent",
        "x-oai-attestation",
        "x-openai-fedramp",
        "x-openai-codex-luna-reserve",
        "x-openai-internal-codex-responses-lite",
        "oai-product-sku",
    ],
    prefixes: &["x-codex-"],
};

/// Headers every backend call carries: bearer token, account, originator,
/// static config headers. Source authentication and the channel's own
/// identity headers are never forwarded; `config.allowed_headers` narrows
/// the rest.
pub(super) fn backend_headers(
    config: &CodexConfig,
    account: &Account<'_>,
    source: Option<(&HeaderMap, Option<&HeaderAllowlist>)>,
) -> Result<HeaderMap, ChannelError> {
    let mut headers = match source {
        Some((source, allowlist)) => forwardable(
            source,
            allowlist,
            &["chatgpt-account-id", "originator", "openai-beta"],
        ),
        None => HeaderMap::new(),
    };
    headers.insert(
        header::AUTHORIZATION,
        header_value(&format!("Bearer {}", account.access_token))?,
    );
    if let Some(id) = &account.account_id {
        headers.insert(
            HeaderName::from_static("chatgpt-account-id"),
            header_value(id)?,
        );
    }
    headers.insert(
        HeaderName::from_static("originator"),
        header_value(&config.originator)?,
    );
    let agent = agent::user_agent(&config.originator);
    headers.insert(header::USER_AGENT, header_value(&agent)?);
    for (name, value) in &config.headers {
        headers.insert(
            HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| invalid_config(format!("header `{name}`")))?,
            HeaderValue::from_str(value).map_err(|_| invalid_config(format!("header `{name}`")))?,
        );
    }
    Ok(headers)
}
