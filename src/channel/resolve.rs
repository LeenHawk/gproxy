//! Effective proxy + TLS-fingerprint resolution (policy only).
//!
//! Layering:
//! - **proxy**: per-credential override, else the provider default, else the
//!   global default.
//! - **TLS fingerprint**: per-credential override, else the provider default.
//!
//! These compute the *effective* values; the transport that applies them — a
//! `(proxy, fingerprint)`-keyed upstream-client pool with wreq impersonation
//! ([`crate::http::client::pool`]) — is wired in `failover/attempt`, which
//! resolves these and fails the candidate (no silent downgrade) on a bad target.

use serde_json::Value;

use crate::store::persistence::records::{Credential, Provider};

/// Effective outbound proxy: per-credential override, else the provider default,
/// else the global default.
pub fn effective_proxy(
    cred: &Credential,
    provider: &Provider,
    global: Option<&str>,
) -> Option<String> {
    cred.proxy_url
        .clone()
        .or_else(|| provider.proxy_url.clone())
        .or_else(|| global.map(str::to_string))
}

/// Effective TLS-emulation fingerprint: per-credential override, else the
/// provider default.
pub fn effective_tls_fingerprint(cred: &Credential, provider: &Provider) -> Option<Value> {
    cred.tls_fingerprint
        .clone()
        .or_else(|| provider.tls_fingerprint.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cred(proxy: Option<&str>, tls: Option<Value>) -> Credential {
        Credential {
            id: 1,
            provider_id: 1,
            name: None,
            kind: "api_key".into(),
            secret_json: json!({}),
            weight: 1,
            rpm_limit: None,
            tpm_limit: None,
            proxy_url: proxy.map(str::to_string),
            tls_fingerprint: tls,
            enabled: true,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn provider(proxy: Option<&str>, tls: Option<Value>) -> Provider {
        Provider {
            id: 1,
            name: "p".into(),
            channel: "openai".into(),
            label: None,
            settings_json: json!({}),
            credential_strategy: "round_robin".into(),
            proxy_url: proxy.map(str::to_string),
            tls_fingerprint: tls,
            enabled: true,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn proxy_credential_then_provider_then_global() {
        // credential wins
        assert_eq!(
            effective_proxy(
                &cred(Some("http://cred"), None),
                &provider(Some("http://prov"), None),
                Some("http://global"),
            )
            .as_deref(),
            Some("http://cred")
        );
        // provider next
        assert_eq!(
            effective_proxy(
                &cred(None, None),
                &provider(Some("http://prov"), None),
                Some("http://global"),
            )
            .as_deref(),
            Some("http://prov")
        );
        // global last
        assert_eq!(
            effective_proxy(
                &cred(None, None),
                &provider(None, None),
                Some("http://global")
            )
            .as_deref(),
            Some("http://global")
        );
        assert_eq!(
            effective_proxy(&cred(None, None), &provider(None, None), None),
            None
        );
    }

    #[test]
    fn tls_credential_overrides_provider() {
        assert_eq!(
            effective_tls_fingerprint(
                &cred(None, Some(json!({ "profile": "c" }))),
                &provider(None, Some(json!({ "profile": "p" }))),
            ),
            Some(json!({ "profile": "c" }))
        );
        assert_eq!(
            effective_tls_fingerprint(&cred(None, None), &provider(None, Some(json!("p")))),
            Some(json!("p"))
        );
        assert_eq!(
            effective_tls_fingerprint(&cred(None, None), &provider(None, None)),
            None
        );
    }
}
