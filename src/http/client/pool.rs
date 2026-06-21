//! Lazily-built upstream clients keyed by `(proxy, fingerprint)` (§7.4). Each
//! distinct effective `(proxy, tls_fingerprint)` target gets one lazily-built
//! [`WreqClient`]; the default client serves the no-proxy/no-fingerprint case.
//! An unusable target config is an error — never a silent fallback to the
//! default client, which would bypass the proxy/TLS-profile policy.

use std::sync::Arc;

use dashmap::DashMap;
use serde_json::Value;

use super::fingerprint::{fingerprint_hash, to_emulation};
use super::{ClientError, UpstreamClient, WreqClient};

/// Upstream client pool keyed by `(proxy, fingerprint_hash)`. The default client
/// (already configured with the global proxy, if any) serves the `(None, None)`
/// target; every other distinct target gets one lazily-built [`WreqClient`], and
/// a target whose config is unusable yields [`ClientError::Config`].
pub struct ClientPool {
    default_client: Arc<dyn UpstreamClient>,
    /// Key: `(proxy URL or "", fingerprint hash or "")`.
    by_target: DashMap<(String, String), Arc<dyn UpstreamClient>>,
}

impl ClientPool {
    pub fn new(default_client: Arc<dyn UpstreamClient>) -> Self {
        Self {
            default_client,
            by_target: DashMap::new(),
        }
    }

    /// Resolve the client for an effective `(proxy, fingerprint)` target.
    ///
    /// `(None, None)` → the default client. Otherwise a client is built once per
    /// distinct target (proxy applied + emulation from the fingerprint) and
    /// cached. Config errors FAIL the attempt rather than silently downgrading
    /// to the default client (which has neither the proxy nor the TLS profile):
    /// a malformed proxy URL is [`ClientError::Config`], and so is a PRESENT
    /// fingerprint that maps to no emulation (wrong shape / nothing usable) —
    /// the caller skips the candidate like any upstream connect failure.
    pub fn for_target(
        &self,
        proxy: Option<&str>,
        fingerprint: Option<&Value>,
    ) -> Result<Arc<dyn UpstreamClient>, ClientError> {
        let fp_hash = fingerprint.map(fingerprint_hash).unwrap_or_default();
        if proxy.is_none() && fp_hash.is_empty() {
            return Ok(Arc::clone(&self.default_client));
        }
        let key = (proxy.unwrap_or_default().to_string(), fp_hash);
        if let Some(c) = self.by_target.get(&key) {
            return Ok(Arc::clone(&c));
        }
        // A PRESENT fingerprint must yield an emulation. One that maps to
        // nothing (non-object, comment-only, no usable layer — see
        // `fingerprint::to_emulation`) would silently drop the TLS-profile
        // layer, so it fails the attempt instead.
        let emulation = match fingerprint {
            None => None,
            Some(fp) => match to_emulation(fp) {
                Some(e) => Some(e),
                None => {
                    tracing::warn!(
                        "tls_fingerprint configured but yields no emulation; failing attempt"
                    );
                    return Err(ClientError::Config(
                        "tls_fingerprint configured but yields no usable emulation \
                         (check the fingerprint schema)"
                            .into(),
                    ));
                }
            },
        };
        match WreqClient::with_proxy_and_emulation(proxy, emulation) {
            Ok(c) => {
                let c: Arc<dyn UpstreamClient> = Arc::new(c);
                self.by_target.insert(key, Arc::clone(&c));
                Ok(c)
            }
            Err(e) => {
                tracing::warn!(error = %e, "target client build failed; failing attempt");
                Err(ClientError::Config(format!(
                    "target client build failed: {e}"
                )))
            }
        }
    }

    /// Resolve the client for a channel's built-in `default_emulation` (§7.4),
    /// used when no DB `tls_fingerprint` overrides it. Keyed by `(proxy, "ch:"+id)`
    /// so each channel's built-in profile is built once and shared. Build failure
    /// fails the attempt — same fail-closed policy as [`Self::for_target`].
    pub fn for_channel(
        &self,
        proxy: Option<&str>,
        channel_id: &str,
        emulation: wreq::Emulation,
    ) -> Result<Arc<dyn UpstreamClient>, ClientError> {
        let key = (
            proxy.unwrap_or_default().to_string(),
            format!("ch:{channel_id}"),
        );
        if let Some(c) = self.by_target.get(&key) {
            return Ok(Arc::clone(&c));
        }
        match WreqClient::with_proxy_and_emulation(proxy, Some(emulation)) {
            Ok(c) => {
                let c: Arc<dyn UpstreamClient> = Arc::new(c);
                self.by_target.insert(key, Arc::clone(&c));
                Ok(c)
            }
            Err(e) => {
                tracing::warn!(error = %e, "channel emulation client build failed; failing attempt");
                Err(ClientError::Config(format!(
                    "channel emulation client build failed: {e}"
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::client::{ClientError, RespStream};
    use bytes::Bytes;
    use serde_json::json;

    /// Dummy default client; the pool never sends through it in these tests.
    struct Dummy;

    #[async_trait::async_trait]
    impl UpstreamClient for Dummy {
        async fn send(
            &self,
            _req: http::Request<Bytes>,
        ) -> Result<http::Response<Bytes>, ClientError> {
            unreachable!("test pool never sends")
        }
        async fn send_streaming(
            &self,
            _req: http::Request<Bytes>,
        ) -> Result<(http::StatusCode, http::HeaderMap, RespStream), ClientError> {
            unreachable!("test pool never sends")
        }
    }

    #[test]
    fn pool_keys_distinct_targets() {
        let pool = ClientPool::new(Arc::new(Dummy));
        let fp = json!({"headers": {"user-agent": "x"}});

        // A syntactically valid proxy URL that is never connected to (we only build).
        let a = pool.for_target(Some("http://127.0.0.1:9"), None).unwrap();
        let b = pool
            .for_target(Some("http://127.0.0.1:9"), Some(&fp))
            .unwrap();
        // Distinct targets (fingerprint differs) → distinct client instances.
        assert!(!Arc::ptr_eq(&a, &b));

        // Same key repeated → same cached client instance.
        let a2 = pool.for_target(Some("http://127.0.0.1:9"), None).unwrap();
        assert!(Arc::ptr_eq(&a, &a2));
        let b2 = pool
            .for_target(Some("http://127.0.0.1:9"), Some(&fp))
            .unwrap();
        assert!(Arc::ptr_eq(&b, &b2));

        // (None, None) → the default client (distinct from the built ones).
        let def = pool.for_target(None, None).unwrap();
        assert!(!Arc::ptr_eq(&def, &a));
        assert!(Arc::ptr_eq(&def, &pool.for_target(None, None).unwrap()));
    }

    /// Regression (§7.4 fail-open): an unusable target config must error, not
    /// silently fall back to the default client (policy bypass).
    #[test]
    fn unusable_target_config_errors() {
        let pool = ClientPool::new(Arc::new(Dummy));

        // Malformed proxy URL → client build failure → Err.
        assert!(matches!(
            pool.for_target(Some("not a proxy url"), None),
            Err(ClientError::Config(_))
        ));

        // Present fingerprint that maps to no emulation → Err.
        let fp = json!({ "_note": "no usable layer" });
        assert!(matches!(
            pool.for_target(None, Some(&fp)),
            Err(ClientError::Config(_))
        ));
    }

    /// A channel's built-in emulation is built once per `(proxy, channel)` and
    /// shared; distinct channels get distinct clients, distinct from the default.
    #[test]
    fn for_channel_caches_per_channel() {
        let pool = ClientPool::new(Arc::new(Dummy));
        let emu = || to_emulation(&json!({ "headers": { "user-agent": "x" } })).unwrap();

        let a = pool.for_channel(None, "codex", emu()).unwrap();
        let a2 = pool.for_channel(None, "codex", emu()).unwrap();
        assert!(Arc::ptr_eq(&a, &a2)); // same channel → cached

        let b = pool.for_channel(None, "kiro", emu()).unwrap();
        assert!(!Arc::ptr_eq(&a, &b)); // different channel → distinct

        // Distinct from the default (None, None) client.
        let def = pool.for_target(None, None).unwrap();
        assert!(!Arc::ptr_eq(&def, &a));
    }
}
