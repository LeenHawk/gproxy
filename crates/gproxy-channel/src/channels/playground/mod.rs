//! Google AI Studio Playground (`aistudio.google.com`), using a Google web
//! session rather than a Gemini API key. This is not `playground.google`.
//!
//! Wire source: our 2026-11-17 Chrome capture and the first-party MakerSuite
//! bundle `KvpBpnk2vKA.2018.O`; see `tests/fixtures/playground/README.md`.
//! GenerateContent is a JSPB server stream. Cookies authenticate the Google
//! session; each generation also needs a fresh, content-bound WAA proof.
//! A provider explicitly selects its own proof service. There is no public
//! signing fallback, and all exchanges use the host-assigned outbound client.

mod auth;
mod config;
mod request;
mod schema;
mod stream;
mod wire;

pub use config::{DEFAULT_BASE_URL, PlaygroundConfig};

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind,
    CredentialRefresh, HOST_CONFIG_KEYS, OperationContext, OperationFuture, ProviderView,
};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireResponse};
use serde_json::Value;

pub const ID: &str = "playground";

#[derive(Debug, Default, Clone, Copy)]
pub struct Playground;

impl BaseChannel for Playground {
    fn id(&self) -> &'static str {
        ID
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Google AI Studio Playground",
            credential_kinds: vec!["cookie"],
            login_modes: vec![],
            capabilities: ChannelCapabilities { refresh: true, ..Default::default() },
            config_keys: [
                ConfigKey::optional("base_url", ConfigKeyKind::String, "MakerSuite RPC origin.")
                    .with_placeholder(DEFAULT_BASE_URL),
                ConfigKey::required("proof_server_url", ConfigKeyKind::String,
                    "Your WAA signing service. Receives this credential's Google cookies and request contents; no public fallback.")
                    .with_placeholder("http://127.0.0.1:8081"),
                ConfigKey::optional("timezone", ConfigKeyKind::String, "IANA timezone sent with the Playground request.")
                    .with_placeholder("UTC"),
                ConfigKey::optional(
                    "openai_compatible",
                    ConfigKeyKind::Bool,
                    "Lower OpenAI/Claude JSON Schemas (parametersJsonSchema / responseJsonSchema) and accept toolConfig.",
                ),
            ].into_iter().chain(HOST_CONFIG_KEYS).collect(),
        }
    }

    fn validate_credential_secret(&self, secret: &Value) -> Result<(), ChannelError> {
        auth::Session::parse(secret).map(|_| ())
    }

    fn initial_credential_expiry(&self, _: &Value) -> Option<i64> {
        Some(0)
    }

    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(self)
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
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(request::generate(context, false))
    }

    fn stream_generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(request::generate(context, true))
    }

    fn list_models<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(request::models(context, false))
    }

    fn get_model<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(request::models(context, true))
    }
}
