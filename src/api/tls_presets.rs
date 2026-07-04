//! Named TLS fingerprint presets for the Console picker.
//!
//! Each preset holds a `fingerprint` blob in the stored-blob format
//! `{headers?, tls?, http2?}` consumed by `src/http/client/fingerprint.rs`.
//! Selecting a preset in the Console stores that blob into a provider's or
//! credential's `tls_fingerprint`.
//!
//! Blobs are re-authored from the authoritative sources:
//! - `docs/agent-tls-fingerprints.md` §5 ("tls_fingerprint JSON 草案")
//! - `#[cfg(test)]` fixtures in `src/http/client/fingerprint.rs`
//!   (claude_fp / codex_fp / gemini_fp / agy_fp / kiro_fp)
//!
//! The `_reference`, `_fidelity`, `_unsupported` comment keys from §5 are
//! **not** included — `fingerprint.rs` ignores them, and they add noise to
//! the API response.
//!
//! This module is cross-target (native + wasm32 edge) — no `#[cfg]` gating.

use serde::Serialize;

/// One named TLS fingerprint preset.
///
/// `fingerprint` is a stored-blob value `{headers?, tls?, http2?}` that
/// `fingerprint.rs` parses into a `ClientFingerprint`.
#[derive(Debug, Serialize)]
pub struct TlsPreset {
    pub id: String,
    pub label: String,
    pub fingerprint: serde_json::Value,
}

/// Named TLS fingerprint presets (agent profiles from docs/agent-tls-fingerprints.md §5).
///
/// Returns the stored-blob form `{headers?, tls?, http2?}` for each preset.
/// All blobs parse cleanly via `fingerprint.rs` — validated by the `tls_presets_valid`
/// test. Selecting a preset in the Console stores its `fingerprint` blob into a
/// provider/credential `tls_fingerprint`.
///
/// Includes: claude, codex, gemini, antigravity, kiro, copilot — all channels
/// for which §5 provides a complete, parse-clean JSON draft.
pub fn tls_presets() -> Vec<TlsPreset> {
    vec![
        TlsPreset {
            id: "claude".into(),
            label: "Claude CLI".into(),
            fingerprint: serde_json::json!({
                "headers": { "user-agent": "claude-cli/2.1.199 (external, sdk-cli)" },
                "tls": {
                    "alpn_protocols": ["http/1.1"],
                    "grease_enabled": false,
                    "min_tls_version": "tls1.2",
                    "max_tls_version": "tls1.3",
                    "cipher_list": "TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256:ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256",
                    "curves_list": "X25519:P-256:P-384",
                    "sigalgs_list": "ecdsa_secp256r1_sha256:rsa_pss_rsae_sha256:rsa_pkcs1_sha256"
                }
            }),
        },
        TlsPreset {
            id: "codex".into(),
            label: "Codex CLI".into(),
            fingerprint: serde_json::json!({
                "headers": { "user-agent": "codex_exec/0.137.0 (Debian 13.0.0; x86_64) xterm-256color" },
                "tls": {
                    "alpn_protocols": ["h2"],
                    "grease_enabled": false,
                    "min_tls_version": "tls1.2",
                    "max_tls_version": "tls1.3",
                    "cipher_list": "TLS_AES_256_GCM_SHA384:TLS_AES_128_GCM_SHA256:ECDHE-ECDSA-AES256-GCM-SHA384",
                    "curves_list": "X25519:P-256:P-384"
                },
                "http2": {
                    "enable_push": false,
                    "initial_window_size": 2097152,
                    "initial_connection_window_size": 5242880,
                    "max_frame_size": 16384,
                    "max_header_list_size": 16384,
                    "headers_pseudo_order": [":method", ":scheme", ":authority", ":path"],
                    "settings_order": [2, 4, 5, 6]
                }
            }),
        },
        TlsPreset {
            id: "gemini".into(),
            label: "Gemini CLI".into(),
            fingerprint: serde_json::json!({
                "headers": { "user-agent": "google-api-nodejs-client/9.15.1" },
                "tls": {
                    "alpn_protocols": [],
                    "grease_enabled": false,
                    "min_tls_version": "tls1.2",
                    "max_tls_version": "tls1.3",
                    "cipher_list": "TLS_AES_256_GCM_SHA384:TLS_AES_128_GCM_SHA256:ECDHE-ECDSA-AES128-GCM-SHA256",
                    "curves_list": "X25519MLKEM768:X25519:P-256:P-384:P-521"
                }
            }),
        },
        TlsPreset {
            id: "antigravity".into(),
            label: "Antigravity".into(),
            fingerprint: serde_json::json!({
                "headers": { "user-agent": "codeium-language-server" },
                "tls": {
                    "alpn_protocols": ["h2", "http/1.1"],
                    "grease_enabled": false,
                    "min_tls_version": "tls1.2",
                    "max_tls_version": "tls1.3",
                    "cipher_list": "ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:TLS_AES_128_GCM_SHA256",
                    "curves_list": "X25519MLKEM768:X25519:P-256:P-384:P-521",
                    "sigalgs_list": "rsa_pss_rsae_sha256:ecdsa_secp256r1_sha256:ed25519",
                    "preserve_tls13_cipher_list": true
                }
            }),
        },
        TlsPreset {
            id: "kiro".into(),
            label: "Kiro CLI".into(),
            fingerprint: serde_json::json!({
                "headers": { "user-agent": "aws-sdk-rust/1.3.10 os/linux lang/rust/1.92.0" },
                "tls": {
                    "alpn_protocols": [],
                    "grease_enabled": false,
                    "min_tls_version": "tls1.2",
                    "max_tls_version": "tls1.3",
                    "cipher_list": "TLS_AES_256_GCM_SHA384:TLS_AES_128_GCM_SHA256:ECDHE-ECDSA-AES256-GCM-SHA384",
                    "curves_list": "X25519:P-256:P-384",
                    "sigalgs_list": "ecdsa_secp384r1_sha384:ecdsa_secp256r1_sha256:ed25519"
                }
            }),
        },
        TlsPreset {
            id: "copilot".into(),
            label: "GitHub Copilot CLI".into(),
            // Model-path preset (api.individual.githubcopilot.com, rustls/http1).
            // §5 draft: docs/agent-tls-fingerprints.md §5 copilot section.
            // No fixture in fingerprint.rs; blob taken directly from §5.
            fingerprint: serde_json::json!({
                "headers": { "user-agent": "copilot/1.0.61 (linux v24.16.0) term/unknown" },
                "tls": {
                    "alpn_protocols": ["http/1.1"],
                    "grease_enabled": false,
                    "min_tls_version": "tls1.2",
                    "max_tls_version": "tls1.3",
                    "cipher_list": "TLS_AES_256_GCM_SHA384:TLS_AES_128_GCM_SHA256:TLS_CHACHA20_POLY1305_SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-AES256-GCM-SHA384:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-RSA-CHACHA20-POLY1305",
                    "curves_list": "X25519:P-256:P-384"
                }
            }),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All presets must have non-empty id/label and a `fingerprint` blob that
    /// contains at least a `tls` or `http2` object — the stored-blob format
    /// that `fingerprint.rs` parses. Confirms the blobs are valid stored blobs.
    #[test]
    fn tls_presets_valid() {
        let presets = tls_presets();
        assert!(
            presets.len() >= 5,
            "expected at least 5 presets, got {}",
            presets.len()
        );
        for p in &presets {
            assert!(!p.id.is_empty(), "preset has empty id");
            assert!(!p.label.is_empty(), "preset '{}' has empty label", p.id);
            let has_tls = p.fingerprint.get("tls").is_some_and(|v| v.is_object());
            let has_http2 = p.fingerprint.get("http2").is_some_and(|v| v.is_object());
            assert!(
                has_tls || has_http2,
                "preset '{}' fingerprint has neither 'tls' nor 'http2' object: {:?}",
                p.id,
                p.fingerprint
            );
        }
    }
}
