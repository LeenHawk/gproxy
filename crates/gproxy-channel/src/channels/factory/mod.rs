//! Factory: the Droid CLI's account on api.factory.ai.
//!
//! Wire facts follow droid 1.18.0 (`samples/droid-1.18.0`: the Bun bundle
//! split into `x/js/`, and a loopback capture of the unmodified binary run
//! once per built-in model). One origin serves four wires by path —
//! `/api/llm/a/v1/messages` (Claude Messages), `/api/llm/o/v1/responses`,
//! `/api/llm/o/v1/chat/completions` and `/api/llm/g/v1/generate` (a Gemini
//! body plus `model`, answered as SSE) — and the model decides which one
//! (`models.rs`). The upstream behind it is named by `x-api-provider`; the
//! body keeps Factory's model id and the server maps it.
//!
//! A credential is a Factory API key (`fk-…`, app.factory.ai/settings/api-keys)
//! or a WorkOS access token from the CLI's device login (`oauth.rs`); both
//! are a bearer, and a login that acts in an organization adds
//! `X-Factory-Org-Id`. Nothing on the inference path is signed.
//!
//! Factory publishes no model catalogue to the CLI, so the list is the
//! built-in table, answered locally. Usage windows come from
//! `/api/billing/limits` (`quota.rs`). Credits are computed by the CLI from
//! token counts and are not on the wire, so nothing beyond standard usage is
//! read from replies.
//!
//! **Not covered**: the Responses WebSocket (`/api/llm/o/v1/responses/ws`),
//! which `droid exec` does not use, so no frame of it was captured; and the
//! client-side failover across a model's alternative `x-api-provider`s.

mod config;
mod models;
mod oauth;
mod quota;
mod request;
mod tools;

pub use config::{
    DEFAULT_BASE_URL, DEFAULT_CLIENT_ID, DEFAULT_CLIENT_VERSION, DEFAULT_WORKOS_BASE_URL,
    FactoryConfig, ID,
};
pub use quota::QUOTA_SOURCE;
pub use request::CLIENT_HEADERS;

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, HOST_CONFIG_KEYS, OperationContext,
    PrepareContext, QuotaQuery, Rejection, RejectionContext, RejectionKind, read_body,
    standard_rejection,
};
use gproxy_protocol::connection::Bytes;
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireResponse};
use gproxy_upstream::{
    ConfigKey, ConfigKeyKind, CredentialRefresh, LoginMode, OAuthDeviceCode, ProviderView,
    UpstreamError, UpstreamFuture, UpstreamKind,
};
use http::{HeaderMap, HeaderValue, StatusCode, header};

#[derive(Debug, Default, Clone, Copy)]
pub struct Factory;

impl UpstreamKind for Factory {
    fn id(&self) -> &'static str {
        ID
    }

    fn oauth_device_code(&self) -> Option<&dyn OAuthDeviceCode> {
        Some(self)
    }

    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(self)
    }
}

const fn generation(operation: Operation) -> bool {
    matches!(
        operation,
        Operation::GenerateContent | Operation::StreamGenerateContent
    )
}

impl BaseChannel for Factory {
    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Factory Droid",
            credential_kinds: vec!["api_key", "oauth"],
            login_variants: self.login_variants(),
            login_modes: vec![LoginMode::ApiKey, LoginMode::DeviceCode],
            capabilities: ChannelCapabilities {
                refresh: true,
                quota_query: true,
                ..Default::default()
            },
            config_keys: [
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "Factory API origin; https://api.eu.factory.ai for EU accounts. Provider column, not config JSON.",
                )
                .with_placeholder(DEFAULT_BASE_URL),
                ConfigKey::optional(
                    "workos_base_url",
                    ConfigKeyKind::String,
                    "Where the device login and the refresh talk.",
                )
                .with_placeholder(DEFAULT_WORKOS_BASE_URL),
                ConfigKey::optional(
                    "client_id",
                    ConfigKeyKind::String,
                    "WorkOS client id of the device login.",
                )
                .with_placeholder(DEFAULT_CLIENT_ID),
                ConfigKey::optional(
                    "client_version",
                    ConfigKeyKind::String,
                    "Droid CLI version reported in User-Agent and x-client-version.",
                )
                .with_placeholder(DEFAULT_CLIENT_VERSION),
                ConfigKey::optional(
                    "api_providers",
                    ConfigKeyKind::Json,
                    "Model id to x-api-provider value (anthropic, openai, google, xai, fireworks, baseten, mistral, ...), over the built-in table.",
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

    fn local_operations(&self) -> &'static [Operation] {
        &[Operation::ListModels]
    }

    /// Without a model: every wire the origin serves.
    fn native_dialects(&self, provider: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        self.native_dialects_for_model(provider, operation, None)
    }

    /// One wire per model. The Gemini path always streams, so a buffered
    /// Gemini request is collected by the host from the stream.
    fn native_dialects_for_model(
        &self,
        _: ProviderView<'_>,
        operation: Operation,
        model: Option<&str>,
    ) -> Vec<Dialect> {
        match (operation, model) {
            (Operation::ListModels, _) => vec![Dialect::OpenAiChat],
            (Operation::GenerateContent, None) => {
                vec![Dialect::Claude, Dialect::OpenAi, Dialect::OpenAiChat]
            }
            (Operation::StreamGenerateContent, None) => vec![
                Dialect::Claude,
                Dialect::OpenAi,
                Dialect::OpenAiChat,
                Dialect::Gemini,
            ],
            (Operation::GenerateContent, Some(_)) if request::is_gemini(model) => Vec::new(),
            (operation, Some(_)) if generation(operation) => vec![request::dialect(model)],
            _ => Vec::new(),
        }
    }

    fn default_conversion_target_for_model(
        &self,
        _: ProviderView<'_>,
        source: OperationKey,
        model: Option<&str>,
    ) -> Option<OperationKey> {
        let model = model?;
        generation(source.operation).then(|| OperationKey {
            operation: if request::is_gemini(Some(model)) {
                Operation::StreamGenerateContent
            } else {
                source.operation
            },
            dialect: request::dialect(Some(model)),
        })
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, UpstreamError> {
        request::build(ctx)
    }

    /// The built-in table; Factory has no catalogue endpoint for the CLI.
    fn list_models<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> UpstreamFuture<'a, WireResponse<HttpBody>> {
        Box::pin(async move {
            if context.dialect != Dialect::OpenAiChat {
                return Err(UpstreamError::UnsupportedOperation(OperationKey {
                    operation: Operation::ListModels,
                    dialect: context.dialect,
                }));
            }
            let body = serde_json::to_vec(&models::list())
                .map_err(|error| UpstreamError::InvalidResponse(error.to_string()))?;
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            Ok(WireResponse {
                status: StatusCode::OK,
                headers,
                body: HttpBody::Bytes(Bytes::from(body)),
            })
        })
    }

    fn generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> UpstreamFuture<'a, WireResponse<HttpBody>> {
        Box::pin(send(self, Operation::GenerateContent, context))
    }

    fn stream_generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> UpstreamFuture<'a, WireResponse<HttpBody>> {
        Box::pin(send(self, Operation::StreamGenerateContent, context))
    }

    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }

    /// Factory's own refusal codes (`x/js/0072.js`), then the standard reading.
    fn classify_rejection(&self, context: RejectionContext<'_>) -> Option<Rejection> {
        let body = String::from_utf8_lossy(context.body);
        if body.contains("no_active_subscription") || body.contains("No active subscription") {
            return Some(Rejection::new(RejectionKind::Credit));
        }
        if body.contains("content_policy_rejected") {
            return Some(Rejection::new(RejectionKind::Request));
        }
        if body.contains("requires a different inference provider") {
            return Some(Rejection {
                model_scoped: true,
                ..Rejection::new(RejectionKind::ModelUnserved)
            });
        }
        standard_rejection(context)
    }
}

/// The model decides the `x-api-provider`, so the body is read before
/// preparation even when the host forwards it unbuffered. The reply's tool
/// calls come back under the laundered names (`tools.rs`); they are decoded
/// to the client's own names before the host reads the response.
async fn send(
    channel: &Factory,
    operation: Operation,
    context: OperationContext<'_>,
) -> Result<WireResponse<HttpBody>, UpstreamError> {
    let dialect = context.dialect;
    let client = context.client;
    let mut request = context.request;
    request.body = HttpBody::Bytes(read_body(request.body).await?);
    let prepared = channel.prepare(PrepareContext {
        provider: context.provider,
        credential: context.credential,
        operation: OperationKey { operation, dialect },
        request,
        endpoint_override: context.endpoint_override,
    })?;
    let mut response = client.send(prepared).await?;
    if response.status.is_success() {
        let streaming = response
            .headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("event-stream"));
        response.body = if streaming {
            tools::decode_stream(response.body, dialect)
        } else {
            let bytes = read_body(response.body).await?;
            HttpBody::Bytes(tools::decode_buffered(bytes, dialect))
        };
    }
    Ok(response)
}
