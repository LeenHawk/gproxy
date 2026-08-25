use bytes::Bytes;
use http::{Response, StatusCode};
use serde_json::json;

use crate::dto::{TlsFingerprintDto, TlsPresetDto};
use crate::{AdminError, State, response};

pub(super) fn channels(state: &impl State) -> Result<Response<Bytes>, AdminError> {
    response::json(StatusCode::OK, &state.channel_catalogue())
}

pub(super) fn tls_presets() -> Result<Response<Bytes>, AdminError> {
    response::json(StatusCode::OK, &presets())
}

fn presets() -> Vec<TlsPresetDto> {
    vec![
        TlsPresetDto {
            id: "claude".into(),
            label: "Claude CLI".into(),
            fingerprint: fingerprint(json!({
                "headers": {"user-agent": "claude-cli/2.1.112 (external, cli)"},
                "tls": {
                    "alpn_protocols": ["http/1.1"],
                    "grease_enabled": false,
                    "min_tls_version": "tls1.2",
                    "max_tls_version": "tls1.3",
                    "cipher_list": "TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256:ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256",
                    "curves_list": "X25519:P-256:P-384"
                }
            })),
        },
        TlsPresetDto {
            id: "codex".into(),
            label: "Codex CLI".into(),
            fingerprint: fingerprint(json!({
                "headers": {
                    "user-agent": "codex_exec/0.144.0 (Debian 13.0.0; x86_64) xterm-256color",
                    "originator": "codex_exec"
                },
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
            })),
        },
        preset(
            "gemini",
            "Gemini CLI",
            "google-api-nodejs-client/9.15.1",
            "X25519MLKEM768:X25519:P-256:P-384:P-521",
        ),
        preset(
            "antigravity",
            "Antigravity",
            "codeium-language-server",
            "X25519MLKEM768:X25519:P-256:P-384:P-521",
        ),
        preset(
            "kiro",
            "Kiro CLI",
            "aws-sdk-rust/1.3.10 os/linux lang/rust/1.92.0",
            "X25519:P-256:P-384",
        ),
        preset(
            "copilot",
            "GitHub Copilot CLI",
            "copilot/1.0.61 (linux v24.16.0) term/unknown",
            "X25519:P-256:P-384",
        ),
    ]
}

fn preset(id: &str, label: &str, user_agent: &str, curves: &str) -> TlsPresetDto {
    TlsPresetDto {
        id: id.into(),
        label: label.into(),
        fingerprint: fingerprint(json!({
            "headers": {"user-agent": user_agent},
            "tls": {
                "alpn_protocols": [],
                "grease_enabled": false,
                "min_tls_version": "tls1.2",
                "max_tls_version": "tls1.3",
                "cipher_list": "TLS_AES_256_GCM_SHA384:TLS_AES_128_GCM_SHA256:ECDHE-ECDSA-AES128-GCM-SHA256",
                "curves_list": curves
            }
        })),
    }
}

fn fingerprint(value: serde_json::Value) -> TlsFingerprintDto {
    serde_json::from_value(value).expect("built-in TLS fingerprint preset matches its DTO")
}
