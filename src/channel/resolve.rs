//! Effective proxy + TLS-fingerprint resolution (policy only).
//!
//! Layering:
//! - **proxy**: per-credential override, else the provider default, else the
//!   global default.
//! - **TLS fingerprint**: per-credential override, else the provider default.
//!
//! These compute the *effective* values; the transport that applies them — a
//! `(proxy, fingerprint)`-keyed upstream-client pool with wreq impersonation
//! (the internal `crate::http::client::pool`) — is wired in `failover/attempt`, which
//! resolves these and fails the candidate (no silent downgrade) on a bad target.

use std::collections::BTreeMap;

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
/// provider default. Borrowed — the fingerprint JSON is never cloned here.
pub fn effective_tls_fingerprint<'a>(
    cred: &'a Credential,
    provider: &'a Provider,
) -> Option<&'a Value> {
    cred.tls_fingerprint
        .as_ref()
        .or(provider.tls_fingerprint.as_ref())
}

/// A credential's effective TLS fingerprint PAIRED with its pool hash,
/// resolved and hashed once per snapshot rebuild (§7.4). The pair stays
/// together so an attempt can never mix a stale candidate's fingerprint with
/// a newer snapshot's hash (which would poison the client pool cache).
pub struct TlsTarget {
    pub fingerprint: Value,
    pub hash: String,
}

/// Resolve + hash the effective fingerprint for one credential (snapshot
/// build time). `None` = no fingerprint configured anywhere.
pub fn resolve_tls_target(cred: &Credential, provider: &Provider) -> Option<TlsTarget> {
    let fp = effective_tls_fingerprint(cred, provider)?;
    Some(TlsTarget {
        fingerprint: fp.clone(),
        hash: fingerprint_hash(fp),
    })
}

/// blake3 hex of the canonicalized fingerprint — the pool cache key. Object keys
/// are recursively sorted so two semantically-equal fingerprints that differ only
/// in key insertion order hash identically (and thus share one upstream client).
pub fn fingerprint_hash(fp: &Value) -> String {
    let canonical = canonicalize(fp);
    // Canonical form serializes deterministically (BTreeMap → sorted keys).
    let bytes = serde_json::to_vec(&canonical).unwrap_or_default();
    blake3::hash(&bytes).to_hex().to_string()
}

/// Recursively rebuild `v` with every object's keys sorted, so serialization is
/// order-independent. Arrays keep their order (order is semantic there).
fn canonicalize(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let sorted: BTreeMap<String, Value> = map
                .iter()
                .map(|(k, val)| (k.clone(), canonicalize(val)))
                .collect();
            // serde_json::Map preserves BTreeMap's sorted iteration order on collect.
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(canonicalize).collect()),
        other => other.clone(),
    }
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
            Some(&json!({ "profile": "c" }))
        );
        assert_eq!(
            effective_tls_fingerprint(&cred(None, None), &provider(None, Some(json!("p")))),
            Some(&json!("p"))
        );
        assert_eq!(
            effective_tls_fingerprint(&cred(None, None), &provider(None, None)),
            None
        );
    }

    #[test]
    fn fingerprint_hash_canonical() {
        // Same content, different key insertion order (top-level and nested) → equal hash.
        let a = json!({
            "headers": {"user-agent": "x", "accept": "y"},
            "tls": {"min": 1, "max": 2}
        });
        let b = json!({
            "tls": {"max": 2, "min": 1},
            "headers": {"accept": "y", "user-agent": "x"}
        });
        assert_eq!(fingerprint_hash(&a), fingerprint_hash(&b));

        // Different content → different hash.
        let c = json!({"headers": {"user-agent": "z"}});
        assert_ne!(fingerprint_hash(&a), fingerprint_hash(&c));
    }

    #[test]
    fn tls_target_pairs_fingerprint_with_its_hash() {
        let fp = json!({ "headers": { "user-agent": "x" } });
        let t = resolve_tls_target(&cred(None, Some(fp.clone())), &provider(None, None)).unwrap();
        assert_eq!(t.fingerprint, fp);
        assert_eq!(t.hash, fingerprint_hash(&fp));
        assert!(resolve_tls_target(&cred(None, None), &provider(None, None)).is_none());
    }
}
