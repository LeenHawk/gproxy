//! The two tokens a Copilot credential holds and the GitHub calls that spend
//! the long-lived one.
//!
//! The credential a person acquires is a GitHub OAuth token. It is not what
//! Chat Completions accepts: that wants a Copilot token, minted from the
//! GitHub one at `POST api.github.com/copilot_internal/v2/token` and good for
//! minutes (v3 `copilotcli/auth.rs`). The GitHub token is therefore the
//! refresh token of this channel and the Copilot token its access token; the
//! mint is `CredentialRefresh`, and `prepare` only ever reads back what the
//! last refresh wrote.
//!
//! v3 stored the pair as `github_token` and `copilot_token`; both names are
//! still read so a v3 secret works unchanged.

use super::config::{AccountType, CopilotCliConfig};
use crate::channel::{ChannelError, CredentialView, ProviderView};
use http::{HeaderMap, HeaderName, HeaderValue, header};
use serde_json::Value;

fn text(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// The long-lived GitHub OAuth token, under either name.
pub(super) fn github_token<'a>(credential: &CredentialView<'a>) -> Result<&'a str, ChannelError> {
    ["refresh_token", "github_token"]
        .into_iter()
        .find_map(|name| text(credential.secret.get(name)))
        .ok_or(ChannelError::InvalidCredential)
}

/// The short-lived Copilot token, under either name. A credential that has
/// none is not usable yet: the host's first refresh mints it.
pub(super) fn copilot_token<'a>(credential: &CredentialView<'a>) -> Result<&'a str, ChannelError> {
    ["access_token", "copilot_token"]
        .into_iter()
        .find_map(|name| text(credential.secret.get(name)))
        .ok_or(ChannelError::InvalidCredential)
}

/// The seat this request is served from: the provider column, then the
/// operator's stated seat, then one a host recorded on the credential, then
/// an individual's.
pub(super) fn base_url(
    config: &CopilotCliConfig,
    provider: ProviderView<'_>,
    credential: &CredentialView<'_>,
) -> String {
    if let Some(base) = provider.base_url.map(str::trim).filter(|b| !b.is_empty()) {
        return base.trim_end_matches('/').to_owned();
    }
    config
        .account_type
        .or_else(|| {
            text(credential.metadata.get("account_type"))
                .or_else(|| text(credential.secret.pointer("/provider_fields/account_type")))
                .or_else(|| text(credential.secret.get("account_type")))
                .and_then(AccountType::parse)
        })
        .unwrap_or_default()
        .base_url()
        .to_owned()
}

/// The headers a call to api.github.com carries: the long-lived `token`
/// scheme (not the Copilot bearer) plus the editor fingerprint that makes
/// GitHub treat the caller as an entitled Copilot client.
pub(super) fn github_headers(
    config: &CopilotCliConfig,
    github_token: &str,
) -> Result<HeaderMap, ChannelError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("token {github_token}"))
            .map_err(|_| ChannelError::InvalidCredential)?,
    );
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(
        header::USER_AGENT,
        HeaderValue::from_static(super::config::CLI_USER_AGENT),
    );
    for (name, value) in [
        (
            "editor-version",
            format!("vscode/{}", config.vscode_version()),
        ),
        (
            "editor-plugin-version",
            super::config::EDITOR_PLUGIN_VERSION.to_owned(),
        ),
        (
            "x-github-api-version",
            super::config::GITHUB_API_VERSION.to_owned(),
        ),
    ] {
        headers.insert(
            HeaderName::from_static(name),
            HeaderValue::from_str(&value)
                .map_err(|_| ChannelError::InvalidConfig(format!("header `{name}`")))?,
        );
    }
    Ok(headers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn view<'a>(secret: &'a Value, metadata: &'a Value) -> CredentialView<'a> {
        CredentialView {
            id: "c",
            provider_id: "p",
            auth_kind: "oauth",
            secret,
            metadata,
            version: 1,
            expires_at_ms: None,
        }
    }

    #[test]
    fn both_the_v4_and_the_v3_names_read_the_same_pair() {
        let v4 = json!({"access_token": "copilot", "refresh_token": "github"});
        let v3 = json!({"copilot_token": "copilot", "github_token": "github"});
        let metadata = Value::Null;
        for secret in [&v4, &v3] {
            let credential = view(secret, &metadata);
            assert_eq!(copilot_token(&credential).unwrap(), "copilot");
            assert_eq!(github_token(&credential).unwrap(), "github");
        }
        let empty = json!({});
        assert!(copilot_token(&view(&empty, &metadata)).is_err());
    }

    #[test]
    fn the_seat_decides_the_origin_and_the_provider_column_decides_it_first() {
        let secret = json!({"access_token": "t", "refresh_token": "g"});
        let metadata = json!({"account_type": "enterprise"});
        let config = Value::Object(Default::default());
        let provider = |base| ProviderView {
            id: "p",
            channel: super::super::config::ID,
            base_url: base,
            config: &config,
        };
        let stated = CopilotCliConfig {
            account_type: Some(AccountType::Business),
            ..CopilotCliConfig::default()
        };
        assert_eq!(
            base_url(&stated, provider(None), &view(&secret, &Value::Null)),
            "https://api.business.githubcopilot.com"
        );
        assert_eq!(
            base_url(
                &CopilotCliConfig::default(),
                provider(None),
                &view(&secret, &metadata)
            ),
            "https://api.enterprise.githubcopilot.com"
        );
        assert_eq!(
            base_url(
                &CopilotCliConfig::default(),
                provider(None),
                &view(&secret, &Value::Null)
            ),
            "https://api.githubcopilot.com"
        );
        assert_eq!(
            base_url(
                &stated,
                provider(Some("https://proxy.example/")),
                &view(&secret, &metadata)
            ),
            "https://proxy.example"
        );
    }
}
