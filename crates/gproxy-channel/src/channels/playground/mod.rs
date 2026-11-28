//! Google AI Studio Playground (`aistudio.google.com`), using a Google web
//! session rather than a Gemini API key. This is not `playground.google`.
//!
//! Wire source: our 2026-11-17 Chrome capture and the first-party MakerSuite
//! bundle `KvpBpnk2vKA.2018.O`; see `tests/fixtures/playground/README.md`.
//! GenerateContent is a JSPB server stream. Cookies authenticate the Google
//! session; each generation also needs a fresh, content-bound WAA proof, minted
//! in-process by [`waa`] on a QuickJS-hosted Firefox DOM (native only — QuickJS
//! is C and cannot target wasm32, so this channel is unavailable on the edge
//! build). Outbound requests impersonate Firefox so they match the proof's
//! environment.

// On wasm32 generation is refused up front, so the code behind it is unused there.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

mod auth;
mod config;
mod request;
mod schema;
#[cfg(not(target_arch = "wasm32"))]
mod signer;
mod stream;
#[cfg(not(target_arch = "wasm32"))]
mod waa;
mod wire;

pub use config::{DEFAULT_BASE_URL, PlaygroundConfig};

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, HOST_CONFIG_KEYS, OperationContext,
};
use gproxy_client::{ConnectionConfig, EmulationConfig};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireResponse};
use gproxy_upstream::{
    ConfigKey, ConfigKeyKind, CredentialRefresh, ProviderView, UpstreamError, UpstreamFuture,
    UpstreamKind,
};
use serde_json::Value;

pub const ID: &str = "playground";

#[derive(Debug, Default, Clone, Copy)]
pub struct Playground;

impl UpstreamKind for Playground {
    fn id(&self) -> &'static str {
        ID
    }

    fn validate_credential_secret(&self, secret: &Value) -> Result<(), UpstreamError> {
        auth::Session::parse(secret).map(|_| ())
    }

    fn initial_credential_expiry(&self, _: &Value) -> Option<i64> {
        Some(0)
    }

    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(self)
    }

    /// Impersonate Firefox's TLS/HTTP2 fingerprint so the transport matches the
    /// Firefox environment the WAA proof attests. `headers: false` — we send only
    /// the channel's explicit grpc-web RPC headers (injecting Firefox's browser
    /// header set diverges from the headers Google accepts for this RPC). wreq-util
    /// tops out at 151; the UA header is set to 152 (the profile's) in `request`.
    /// An explicit host/credential profile still overrides this.
    fn default_connection(&self) -> Option<ConnectionConfig> {
        Some(ConnectionConfig {
            emulation: Some(EmulationConfig::Preset {
                profile: "firefox_151".into(),
                platform: "windows".into(),
                http2: true,
                headers: false,
            }),
            ..ConnectionConfig::default()
        })
    }
}

impl BaseChannel for Playground {
    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Google AI Studio Playground",
            credential_kinds: vec!["cookie"],
            login_variants: self.login_variants(),
            login_modes: vec![],
            capabilities: ChannelCapabilities {
                refresh: true,
                ..Default::default()
            },
            config_keys: [
                ConfigKey::optional("base_url", ConfigKeyKind::String, "MakerSuite RPC origin.")
                    .with_placeholder(DEFAULT_BASE_URL),
                ConfigKey::optional(
                    "timezone",
                    ConfigKeyKind::String,
                    "IANA timezone sent with the Playground request.",
                )
                .with_placeholder("UTC"),
            ]
            .into_iter()
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }

    fn native_dialects(&self, _: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        // CountTokens intentionally has no upstream dialect: Core's default
        // route uses its configured local tokenizer, before credential/WAA I/O.
        match operation {
            Operation::GenerateContent
            | Operation::StreamGenerateContent
            | Operation::ListModels
            | Operation::GetModel => vec![Dialect::Gemini],
            _ => vec![],
        }
    }

    fn default_conversion_target(
        &self,
        provider: ProviderView<'_>,
        source: OperationKey,
    ) -> Option<OperationKey> {
        (source.dialect != Dialect::Gemini
            && !self.native_dialects(provider, source.operation).is_empty())
        .then_some(OperationKey {
            dialect: Dialect::Gemini,
            ..source
        })
    }

    fn generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> UpstreamFuture<'a, WireResponse<HttpBody>> {
        Box::pin(request::generate(context, false))
    }

    fn stream_generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> UpstreamFuture<'a, WireResponse<HttpBody>> {
        Box::pin(request::generate(context, true))
    }

    fn list_models<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> UpstreamFuture<'a, WireResponse<HttpBody>> {
        Box::pin(request::models(context, false))
    }

    fn get_model<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> UpstreamFuture<'a, WireResponse<HttpBody>> {
        Box::pin(request::models(context, true))
    }
}
