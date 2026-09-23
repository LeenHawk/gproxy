//! GitHub Copilot CLI: a GitHub account's Copilot seat, reached the way the
//! `copilot` CLI reaches it.
//!
//! Wire facts follow the v3 channel (`crates/gproxy-channels/src/copilotcli`
//! on `main`). The credential a person acquires is a GitHub OAuth token from
//! GitHub's device flow, and it is *not* what the Copilot backend accepts:
//! Chat Completions wants a Copilot token, minted from the GitHub one at
//! `POST api.github.com/copilot_internal/v2/token` and good for minutes.
//! That exchange is modelled as `CredentialRefresh` (`oauth.rs`) — a mint of
//! a short-lived credential from a long-lived one is a refresh, and
//! `prepare`, being synchronous and pure, would otherwise re-mint on every
//! request. The login's final step performs the first mint so the persisted
//! credential is usable at once.
//!
//! The secret is therefore `{access_token: <copilot>, refresh_token:
//! <github>}`; v3's `copilot_token` / `github_token` names are still read, and
//! a refresh keeps them in step when it finds them.
//!
//! Two origins, two identities. Calls to `api.github.com` carry the
//! long-lived `token` scheme with the editor fingerprint whose entitlement
//! mints Copilot tokens (`auth.rs`); calls to the Copilot backend carry the
//! short-lived bearer with the CLI's own identity — integration id, editor
//! version, intent, a machine id derived from the credential and a fresh
//! interaction id, plus `x-initiator`, which says whether a person or the
//! agent itself is asking (`identity.rs`). Which inference origin is reached
//! follows the seat: individual, business or enterprise.
//!
//! **v3 infrastructure that has no v4 counterpart.** v3's `routes.rs`
//! (`SurfaceTable` written with the `route!` macro) is replaced by
//! `native_dialects` plus the host's protocol conversion. v3's `ChannelLogin`
//! becomes `OAuthDeviceCode`. v3's `StreamDecoder` is gone: the backend
//! answers Chat Completions SSE verbatim, so only `UsageStream` watches the
//! bytes. v3's `model.rs` rewrote the body only to substitute the upstream
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
mod usage;

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
    QuotaQuery, UsageExtractor, UsageStream,
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
            ocsp_stapling: None,
            signed_cert_timestamps: None,
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
            login_modes: vec![LoginMode::DeviceCode],
            capabilities: ChannelCapabilities {
                refresh: true,
                quota_query: true,
                quota_reset: false,
                services: false,
                websocket: false,
            },
            config_keys: [
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "Copilot inference origin; defaults to the one the seat implies. Provider column, not config JSON.",
                ),
                ConfigKey::optional(
                    "account_type",
                    ConfigKeyKind::String,
                    "`individual`, `business` or `enterprise`. Decides the inference origin when base_url names none.",
                ),
                ConfigKey::optional(
                    "github_api_url",
                    ConfigKeyKind::String,
                    "Where the Copilot token is minted and the seat probed; defaults to https://api.github.com.",
                ),
                ConfigKey::optional(
                    "device_authorization_url",
                    ConfigKeyKind::String,
                    "Where the device login starts; defaults to GitHub's device code endpoint.",
                ),
                ConfigKey::optional(
                    "token_url",
                    ConfigKeyKind::String,
                    "Where the device login polls for the GitHub token.",
                ),
                ConfigKey::optional(
                    "scope",
                    ConfigKeyKind::String,
                    "Scope the device login asks for; defaults to read:user.",
                ),
                ConfigKey::optional(
                    "vscode_version",
                    ConfigKeyKind::String,
                    "Editor release named in editor-version on GitHub calls.",
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

    fn usage_extractor(&self) -> Option<&dyn UsageExtractor> {
        Some(self)
    }

    fn usage_stream(&self) -> Option<&dyn UsageStream> {
        Some(self)
    }
}
