//! Alibaba DashScope: one origin, four API layouts.
//!
//! Wire facts follow DashScope's API reference and the v3 `dashscope`
//! channel. `/compatible-mode/v1/...` serves the OpenAI-compatible surface
//! (chat, responses, embeddings, models), `/apps/anthropic/v1/messages` the
//! Anthropic-compatible one, `/compatible-api/v1/reranks` reranking, and
//! `/api/v1/services/aigc/multimodal-generation/generation` the native
//! image API, which is not OpenAI-shaped in either direction (`image.rs`).
//! Every surface takes the key as `Authorization: Bearer`, the
//! Anthropic-compatible one included, and none of them wants an
//! `anthropic-version`.
//!
//! Not ported from v3: the account balance, which is not a DashScope
//! endpoint at all but a signed Alibaba Cloud BSS `QueryAccountBalance` RPC
//! against `business.aliyuncs.com` with its own access key pair. It needs a
//! cloud credential rather than the model key and a request signer this
//! crate has no other use for.

mod config;
mod image;
mod request;
mod usage;

pub use config::{DEFAULT_BASE_URL, DashScopeConfig, ID};

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind,
    HOST_CONFIG_KEYS, LoginMode, OperationContext, OperationFuture, PrepareContext, ProviderView,
    UsageExtractor, UsageStream,
};
use crate::channels::shared::compatible::http::read_body;
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireResponse};

#[derive(Debug, Default, Clone, Copy)]
pub struct DashScope;

impl DashScope {
    /// Post the native image envelope and fold the reply back into an
    /// OpenAI image response. The exchange is buffered because the envelope
    /// is synchronous: DashScope returns the finished images, not a stream.
    async fn image_call(
        &self,
        operation: Operation,
        ctx: OperationContext<'_>,
    ) -> Result<WireResponse<HttpBody>, ChannelError> {
        let request = self.prepare(PrepareContext {
            provider: ctx.provider,
            credential: ctx.credential,
            operation: OperationKey {
                operation,
                dialect: ctx.dialect,
            },
            request: ctx.request,
            endpoint_override: ctx.endpoint_override,
        })?;
        let WireResponse {
            status,
            mut headers,
            body,
        } = ctx.client.send(request).await?;
        let body = read_body(body).await?;
        if !status.is_success() {
            return Ok(WireResponse {
                status,
                headers,
                body: HttpBody::Bytes(body),
            });
        }
        let body = image::response(body);
        // The rewritten document has a length of its own.
        headers.remove(http::header::CONTENT_LENGTH);
        Ok(WireResponse {
            status,
            headers,
            body: HttpBody::Bytes(body),
        })
    }
}

impl BaseChannel for DashScope {
    fn id(&self) -> &'static str {
        ID
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Alibaba DashScope",
            login_modes: vec![LoginMode::ApiKey],
            capabilities: ChannelCapabilities::default(),
            config_keys: [
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "Upstream origin; defaults to https://dashscope.aliyuncs.com. The compatibility prefixes are appended by the channel. Provider column, not config JSON.",
                ),
                ConfigKey::optional(
                    "headers",
                    ConfigKeyKind::HeaderList,
                    "Static headers added to every upstream request.",
                ),
            ]
            .into_iter()
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }

    fn native_dialects(&self, _: ProviderView<'_>, _: Operation) -> Vec<Dialect> {
        vec![Dialect::OpenAiChat, Dialect::OpenAi, Dialect::Claude]
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let (builder, request) = request::build(ctx)?;
        builder
            .body(request.body)
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    fn create_image<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(self.image_call(Operation::CreateImage, context))
    }

    fn edit_image<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(self.image_call(Operation::EditImage, context))
    }

    fn usage_extractor(&self) -> Option<&dyn UsageExtractor> {
        Some(self)
    }

    fn usage_stream(&self) -> Option<&dyn UsageStream> {
        Some(self)
    }
}
