//! GitHub Copilot CLI: a GitHub account's Copilot seat, reached the way the
//! `copilot` CLI reaches it.
//!
//! Wire facts follow Copilot CLI 1.0.77 (`@github/copilot-linux-x64`:
//! `app.js` and the strings of its `runtime.node`). The credential a person
//! acquires is a GitHub OAuth token from GitHub's device flow with the CLI's
//! own OAuth app, and that token is what every call presents as its bearer:
//! the account probe on api.github.com and Chat Completions on the Copilot
//! origin alike. The CLI mints no short-lived Copilot token.
//!
//! The secret is therefore `{access_token: <github>}`. The login asks
//! `copilot_internal/user` who signed in and where the seat is served
//! (`endpoints.api`), and records those as public facts; the refresh is the
//! same probe, which also learns when GitHub stops honouring the token
//! (`oauth.rs`). A credential from the channel's earlier design — the VS Code
//! Copilot Chat app's GitHub token beside a minted Copilot token, under v4's
//! or v3's names — is refused with a request to sign in again.
//!
//! Calls to the Copilot backend carry the CLI's identity — integration id,
//! intent, a machine id derived from the credential and a fresh interaction
//! id, plus `x-initiator`, which says whether a person or the agent itself
//! is asking (`identity.rs`). The origin is the one the probe reported,
//! unless the operator names one.
//!
//! **v3 infrastructure that has no v4 counterpart.** v3's `routes.rs`
//! (`SurfaceTable` written with the `route!` macro) is replaced by
//! `native_dialects` plus the host's protocol conversion. v3's `ChannelLogin`
//! becomes `OAuthDeviceCode`. v3's `StreamDecoder` is gone: the backend
//! answers Chat Completions SSE verbatim, and the host reads usage from it as
//! it is. v3's `model.rs` rewrote the body only to substitute the upstream
//! model name, which v4's host has already done before `prepare` is reached.
//! v3's `ChannelTrafficPolicy` (no client request header forwarded at all)
//! has no v4 counterpart; an operator who wants it writes
//! `allowed_headers: []`, and the CLI's own headers survive it because they
//! are declared as `CLI_HEADERS`.

mod auth;
mod config;
mod identity;
mod oauth;
mod quota;
mod request;

pub use config::{
    AccountType, BUSINESS_BASE_URL, CopilotCliConfig, DEFAULT_CLIENT_ID,
    DEFAULT_DEVICE_AUTHORIZATION_URL, DEFAULT_GITHUB_API_URL, DEFAULT_TOKEN_URL,
    ENTERPRISE_BASE_URL, ID, INDIVIDUAL_BASE_URL,
};
pub use identity::CLI_HEADERS;
pub use quota::QUOTA_SOURCE;

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind,
    CredentialRefresh, HOST_CONFIG_KEYS, LoginMode, OAuthDeviceCode, PrepareContext, ProviderView,
    QuotaQuery,
};
use gproxy_client::{Alpn, Backend, ConnectionConfig, EmulationConfig, Fingerprint, TlsVersion};
use gproxy_protocol::{Dialect, HttpBody, Operation};

/// The CLI's outbound stack, the default for a provider that names no
/// connection profile: HTTP/1.1 only, a TLS 1.2 floor, the CLI's cipher and
/// curve order and no GREASE (v3 `copilotcli/profile.rs`).
pub fn default_connection() -> ConnectionConfig {
    ConnectionConfig {
        backend: Backend::Wreq,
        emulation: Some(EmulationConfig::Custom(Fingerprint {
            alpn: vec![Alpn::Http1],
            disable_alpn: None,
            min_tls: Some(TlsVersion::Tls12),
            max_tls: Some(TlsVersion::Tls13),
            cipher_list: Some(
                concat!(
                    "TLS_AES_256_GCM_SHA384:TLS_AES_128_GCM_SHA256:TLS_CHACHA20_POLY1305_SHA256:",
                    "ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-ECDSA-AES128-GCM-SHA256:",
                    "ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-AES256-GCM-SHA384:",
                    "ECDHE-RSA-AES128-GCM-SHA256:ECDHE-RSA-CHACHA20-POLY1305"
                )
                .into(),
            ),
            curves_list: Some("X25519:P-256:P-384".into()),
            sigalgs_list: None,
            preserve_tls13_cipher_list: Some(false),
            grease: Some(false),
            extension_permutation: None,
            ocsp_stapling: None,
            signed_cert_timestamps: None,
            session_ticket: None,
            psk_dhe_ke: None,
            http2: None,
            headers: None,
        })),
        ..ConnectionConfig::default()
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CopilotCli;

impl BaseChannel for CopilotCli {
    fn id(&self) -> &'static str {
        ID
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "GitHub Copilot CLI",
            credential_kinds: vec!["oauth"],
            login_modes: vec![LoginMode::DeviceCode],
            capabilities: ChannelCapabilities {
                refresh: true,
                quota_query: true,
                paid_usage: true,
                ..Default::default()
            },
            config_keys: [
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "Copilot inference origin; defaults to the one GitHub reports for the account. Provider column, not config JSON.",
                ),
                ConfigKey::optional(
                    "account_type",
                    ConfigKeyKind::String,
                    "`individual`, `business` or `enterprise`. Overrides the inference origin GitHub reports when base_url names none.",
                ),
                ConfigKey::optional(
                    "github_api_url",
                    ConfigKeyKind::String,
                    "Where the account is probed (copilot_internal/user); defaults to https://api.github.com.",
                ).with_placeholder(config::DEFAULT_GITHUB_API_URL),
                ConfigKey::optional(
                    "device_authorization_url",
                    ConfigKeyKind::String,
                    "Where the device login starts; defaults to GitHub's device code endpoint.",
                ).with_placeholder(config::DEFAULT_DEVICE_AUTHORIZATION_URL),
                ConfigKey::optional(
                    "token_url",
                    ConfigKeyKind::String,
                    "Where the device login polls for the GitHub token.",
                ).with_placeholder(config::DEFAULT_TOKEN_URL),
                ConfigKey::optional(
                    "scope",
                    ConfigKeyKind::String,
                    "Scope the device login asks for; defaults to the CLI's read:user,read:org,repo,gist,codespace.",
                ),
                ConfigKey::optional(
                    "headers",
                    ConfigKeyKind::HeaderList,
                    "Static headers added to every inference request.",
                ),
            ]
            .into_iter()
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }

    fn default_connection(&self) -> Option<ConnectionConfig> {
        Some(default_connection())
    }

    /// The Copilot backend serves Chat Completions and a catalogue; every
    /// other dialect is the host's conversion problem.
    fn native_dialects(&self, _: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent
            | Operation::StreamGenerateContent
            | Operation::ListModels => vec![Dialect::OpenAiChat],
            _ => Vec::new(),
        }
    }

    /// A credential from the channel's earlier design cannot be used: its
    /// GitHub token belongs to another OAuth app.
    fn validate_credential_secret(&self, secret: &serde_json::Value) -> Result<(), ChannelError> {
        if auth::legacy(secret) {
            return Err(ChannelError::InvalidConfig(auth::LEGACY_REASON.into()));
        }
        Ok(())
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        request::build(ctx)
    }

    fn oauth_device_code(&self) -> Option<&dyn OAuthDeviceCode> {
        Some(self)
    }

    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(self)
    }

    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }
    fn quota_model(&self) -> Option<&dyn crate::channel::QuotaModel> {
        Some(self)
    }
}
