//! The one token a Copilot credential holds and where it is spent.
//!
//! The Copilot CLI (1.0.77) signs in with GitHub's device flow and keeps the
//! GitHub OAuth token it is granted. That token is the bearer on every call:
//! the account probe at `api.github.com/copilot_internal/user` and the
//! Copilot inference origin alike (`app.js`: `Authorization: Bearer ${await
//! CFe(e)}`, where `CFe` resolves `authManagerTokenForInfo`, the stored GitHub
//! token). No short-lived Copilot token is minted: `copilot_internal/v2/token`
//! appears nowhere in the CLI.
//!
//! The inference origin is the one the account probe reports in
//! `endpoints.api`, recorded on the credential as `api_url`.

use super::config::{AccountType, CopilotCliConfig, GITHUB_ACCEPT};
use crate::channel::{ChannelError, CredentialView, ProviderView};
use http::{HeaderMap, HeaderValue, header};
use serde_json::Value;

/// The public fact naming the inference origin the account probe reported.
pub(super) const API_URL: &str = "api_url";

fn text(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Whether this secret came from the channel's earlier design: the VS Code
/// Copilot Chat app's GitHub token beside a minted Copilot token (v4 before
/// it followed the CLI) or v3's names for the same pair. Neither can be
/// carried over: the GitHub token belongs to another OAuth app.
pub(super) fn legacy(secret: &Value) -> bool {
    ["refresh_token", "github_token", "copilot_token"]
        .into_iter()
        .any(|name| text(secret.get(name)).is_some())
}

/// The message a credential from the earlier design is refused with.
pub(super) const LEGACY_REASON: &str =
    "re-login required: credential from the old Copilot Chat app";

/// The GitHub OAuth token. A legacy secret has none this channel may use.
pub(super) fn github_token<'a>(credential: &CredentialView<'a>) -> Result<&'a str, ChannelError> {
    if legacy(credential.secret) {
        return Err(ChannelError::InvalidCredential);
    }
    text(credential.secret.get("access_token")).ok_or(ChannelError::InvalidCredential)
}

/// A public fact: host metadata first, then the secret's `provider_fields`.
pub(super) fn fact<'a>(credential: &CredentialView<'a>, name: &str) -> Option<&'a str> {
    text(credential.metadata.get(name)).or_else(|| {
        text(
            credential
                .secret
                .pointer(&format!("/provider_fields/{name}")),
        )
    })
}

/// The origin this request is served from: the provider column, then the
/// operator's stated seat, then the origin the account probe reported, then
/// an individual's.
pub(super) fn base_url(
    config: &CopilotCliConfig,
    provider: ProviderView<'_>,
    credential: &CredentialView<'_>,
) -> String {
    if let Some(base) = provider.base_url.map(str::trim).filter(|b| !b.is_empty()) {
        return base.trim_end_matches('/').to_owned();
    }
    if let Some(seat) = config.account_type {
        return seat.base_url().to_owned();
    }
    fact(credential, API_URL)
        .unwrap_or(AccountType::default().base_url())
        .trim_end_matches('/')
        .to_owned()
}

/// Accept an `endpoints.api` only when it is an https origin.
pub(super) fn api_origin(value: &str) -> Option<String> {
    let value = value.trim().trim_end_matches('/');
    let uri = value.parse::<http::Uri>().ok()?;
    (uri.scheme_str() == Some("https") && uri.authority().is_some()).then(|| value.to_owned())
}

/// The headers a call to api.github.com carries: the GitHub token as a
/// bearer (the CLI's `ZO` request builder: `Authorization: Bearer ${t}`),
/// the REST media type and the CLI's banner.
pub(super) fn github_headers(github_token: &str) -> Result<HeaderMap, ChannelError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {github_token}"))
            .map_err(|_| ChannelError::InvalidCredential)?,
    );
    headers.insert(header::ACCEPT, HeaderValue::from_static(GITHUB_ACCEPT));
    headers.insert(
        header::USER_AGENT,
        HeaderValue::from_static(super::config::CLI_USER_AGENT),
    );
    Ok(headers)
}

#[cfg(test)]
mod tests {
    use super::super::config::INDIVIDUAL_BASE_URL;
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
    fn a_minted_pair_is_legacy_and_a_bare_github_token_is_not() {
        let metadata = Value::Null;
        let current = json!({"access_token": "gho_x", "refresh_token": null});
        assert_eq!(github_token(&view(&current, &metadata)).unwrap(), "gho_x");
        for old in [
            json!({"access_token": "tid=x", "refresh_token": "gho_x"}),
            json!({"copilot_token": "tid=x", "github_token": "gho_x"}),
        ] {
            assert!(legacy(&old));
            assert!(github_token(&view(&old, &metadata)).is_err());
        }
    }

    #[test]
    fn the_operator_then_the_probe_decides_the_origin() {
        let secret = json!({"access_token": "gho_x",
            "provider_fields": {"api_url": "https://api.business.githubcopilot.com"}});
        let config = Value::Object(Default::default());
        let provider = |base| ProviderView {
            id: "p",
            channel: super::super::config::ID,
            base_url: base,
            config: &config,
        };
        let stated = CopilotCliConfig {
            account_type: Some(AccountType::Enterprise),
            ..CopilotCliConfig::default()
        };
        let credential = view(&secret, &Value::Null);
        assert_eq!(
            base_url(&stated, provider(None), &credential),
            "https://api.enterprise.githubcopilot.com"
        );
        assert_eq!(
            base_url(&CopilotCliConfig::default(), provider(None), &credential),
            "https://api.business.githubcopilot.com"
        );
        let bare = json!({"access_token": "gho_x"});
        assert_eq!(
            base_url(
                &CopilotCliConfig::default(),
                provider(None),
                &view(&bare, &Value::Null)
            ),
            INDIVIDUAL_BASE_URL
        );
        assert_eq!(
            base_url(
                &stated,
                provider(Some("https://proxy.example/")),
                &credential
            ),
            "https://proxy.example"
        );
        assert_eq!(api_origin("http://evil.example"), None);
    }
}
