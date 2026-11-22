//! The whole configuration, handed in as JSON when the engine starts.

use std::sync::Arc;

use gproxy_channel::BaseChannel;
use serde::Deserialize;
use serde_json::Value;

/// The management paths live under `/lite`, so no provider may take the name.
const RESERVED_NAME: &str = "lite";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("configuration is not valid JSON: {0}")]
    Json(String),
    #[error("access_key must not be blank")]
    BlankAccessKey,
    #[error("no providers are configured")]
    NoProviders,
    #[error("provider name `{0}` must be letters, digits, `.`, `_` or `-`, and not `lite`")]
    ProviderName(String),
    #[error("provider `{0}` is configured twice")]
    DuplicateProvider(String),
    #[error("provider `{provider}` names channel `{channel}`; this build has: {available}")]
    UnknownChannel {
        provider: String,
        channel: String,
        available: String,
    },
    #[error("the credential of provider `{0}` is not one its channel accepts")]
    Credential(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    access_key: String,
    providers: Vec<RawProvider>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProvider {
    name: String,
    channel: String,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    config: Option<Value>,
    #[serde(default)]
    credential: Option<RawCredential>,
}

/// A pasted API key, or a secret in the channel's own shape with its kind
/// (`service_account`, `cookie`, `oauth`, ...).
#[derive(Deserialize)]
#[serde(untagged)]
enum RawCredential {
    Key(String),
    Secret { kind: String, secret: Value },
}

/// A credential written into the configuration.
#[derive(Clone)]
pub(crate) struct Pasted {
    pub kind: String,
    pub secret: Value,
}

pub(crate) struct Provider {
    pub name: String,
    pub channel: Arc<dyn BaseChannel>,
    pub base_url: Option<String>,
    pub config: Value,
    pub pasted: Option<Pasted>,
}

pub(crate) struct Config {
    pub access_key: String,
    pub providers: Vec<Provider>,
}

impl Config {
    pub fn parse(json: &str, channels: &[Arc<dyn BaseChannel>]) -> Result<Self, ConfigError> {
        let raw: RawConfig =
            serde_json::from_str(json).map_err(|error| ConfigError::Json(error.to_string()))?;
        if raw.access_key.trim().is_empty() {
            return Err(ConfigError::BlankAccessKey);
        }
        if raw.providers.is_empty() {
            return Err(ConfigError::NoProviders);
        }
        let mut providers: Vec<Provider> = Vec::with_capacity(raw.providers.len());
        for provider in raw.providers {
            let name = provider.name;
            let plain = !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
            if !plain || name == RESERVED_NAME {
                return Err(ConfigError::ProviderName(name));
            }
            if providers.iter().any(|known| known.name == name) {
                return Err(ConfigError::DuplicateProvider(name));
            }
            let Some(channel) = channels.iter().find(|c| c.id() == provider.channel) else {
                let mut available: Vec<_> = channels.iter().map(|c| c.id()).collect();
                available.sort_unstable();
                return Err(ConfigError::UnknownChannel {
                    provider: name,
                    channel: provider.channel,
                    available: available.join(", "),
                });
            };
            let pasted = provider.credential.map(|credential| match credential {
                RawCredential::Key(key) => Pasted {
                    kind: "api_key".to_owned(),
                    secret: serde_json::json!({ "api_key": key }),
                },
                RawCredential::Secret { kind, secret } => Pasted { kind, secret },
            });
            if let Some(pasted) = &pasted
                && (!channel.credential_kinds().contains(&pasted.kind.as_str())
                    || channel.validate_credential_secret(&pasted.secret).is_err())
            {
                return Err(ConfigError::Credential(name));
            }
            providers.push(Provider {
                name,
                channel: channel.clone(),
                base_url: provider.base_url.filter(|url| !url.trim().is_empty()),
                config: provider.config.unwrap_or_else(|| serde_json::json!({})),
                pasted,
            });
        }
        Ok(Self {
            access_key: raw.access_key,
            providers,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channels() -> Vec<Arc<dyn BaseChannel>> {
        gproxy_channel::channels::compiled_in()
    }

    fn parse(json: &str) -> Result<Config, ConfigError> {
        Config::parse(json, &channels())
    }

    #[test]
    fn a_pasted_key_and_a_provider_still_to_sign_in() {
        let config = parse(
            r#"{"access_key":"k","providers":[
                {"name":"openai","channel":"openai","base_url":"https://api.openai.com","credential":"sk-test"},
                {"name":"codex","channel":"codex"}
            ]}"#,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(config.providers.len(), 2);
        let pasted = config.providers[0].pasted.as_ref().unwrap();
        assert_eq!(pasted.kind, "api_key");
        assert_eq!(pasted.secret["api_key"], "sk-test");
        assert!(config.providers[1].pasted.is_none());
        assert_eq!(config.providers[1].config, serde_json::json!({}));
    }

    #[test]
    fn what_cannot_serve_a_request_is_refused_at_start() {
        let provider = r#"{"name":"a","channel":"openai"}"#;
        assert_eq!(
            parse(&format!(r#"{{"access_key":" ","providers":[{provider}]}}"#)).err(),
            Some(ConfigError::BlankAccessKey)
        );
        assert_eq!(
            parse(r#"{"access_key":"k","providers":[]}"#).err(),
            Some(ConfigError::NoProviders)
        );
        assert_eq!(
            parse(&format!(
                r#"{{"access_key":"k","providers":[{provider},{provider}]}}"#
            ))
            .err(),
            Some(ConfigError::DuplicateProvider("a".into()))
        );
        for name in ["lite", "a/b", ""] {
            assert_eq!(
                parse(&format!(
                    r#"{{"access_key":"k","providers":[{{"name":"{name}","channel":"openai"}}]}}"#
                ))
                .err(),
                Some(ConfigError::ProviderName(name.into()))
            );
        }
        assert!(matches!(
            parse(r#"{"access_key":"k","providers":[{"name":"a","channel":"nope"}]}"#).err(),
            Some(ConfigError::UnknownChannel { .. })
        ));
        // A pasted key on a channel that only signs in.
        assert_eq!(
            parse(r#"{"access_key":"k","providers":[{"name":"a","channel":"codex","credential":"x"}]}"#)
                .err(),
            Some(ConfigError::Credential("a".into()))
        );
        assert!(matches!(
            parse(r#"{"access_key":"k","providers":[],"users":[]}"#).err(),
            Some(ConfigError::Json(_))
        ));
    }
}
