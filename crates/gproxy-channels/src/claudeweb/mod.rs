mod routes;

mod auth;
mod bootstrap;
mod endpoint;
mod id;
mod login;
mod media;
mod models;
mod orchestrator;
mod prepare;
mod profile;
mod quota;
mod request;
mod stream;

use gproxy_channel_api::{
    BoxFuture, Channel, ChannelDescriptor, ChannelError, ChannelLoginRef, ChannelSupport,
    Disposition, LoginDescriptor, LoginMode, NormalizedUsage, OperationDriver, PrepareCtx,
    PreparedRequest, ResponseView, SimpleHttp, StreamCtx, StreamDecoder, UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey};
use serde_json::Value;

pub struct ClaudeWebChannel;

const fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

static SUPPORTS: [ChannelSupport; 8] = [
    ChannelSupport::passthrough(content(
        Operation::GenerateContent,
        ContentGenerationKind::ClaudeMessages,
    )),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
        content(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
    ),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        content(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
    ),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::GeminiGenerateContent,
        ),
        content(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
    ),
    ChannelSupport::passthrough(content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::ClaudeMessages,
    )),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::GeminiGenerateContent,
        ),
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
    ),
];

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "claudeweb",
    display_name: "Claude Web",
    supports: &SUPPORTS,
    provider_fields: crate::metadata::BASE_URL,
    credential_fields: crate::metadata::CLAUDE_WEB,
    endpoint_overrides: true,
    traffic_policy: crate::policy::CLAUDE_WEB,
};

static LOGIN: LoginDescriptor = LoginDescriptor {
    modes: &[LoginMode::Cookie],
    params: &[],
};

impl Channel for ClaudeWebChannel {
    fn routing_table(&self) -> &'static [ChannelSupport] {
        routes::ROUTES
    }

    fn descriptor(&self) -> &'static ChannelDescriptor {
        &DESCRIPTOR
    }

    fn local_models(&self, secret: &Value) -> Option<Vec<gproxy_channel_api::ModelInfo>> {
        Some(models::from_secret(secret))
    }

    fn login(&self) -> Option<ChannelLoginRef<'_>> {
        Some(ChannelLoginRef {
            adapter: self,
            descriptor: &LOGIN,
        })
    }

    fn prepare(&self, _ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        Err(ChannelError::Prepare(
            "ClaudeWeb operations require orchestration".into(),
        ))
    }

    fn operation_driver(
        &self,
        ctx: PrepareCtx<'_>,
    ) -> Result<Option<Box<dyn OperationDriver>>, ChannelError> {
        orchestrator::driver(ctx)
    }

    fn classify(&self, response: ResponseView<'_>) -> Disposition {
        match response.status.as_u16() {
            200..=299 => Disposition::Success,
            401..=403 => Disposition::CredentialDead,
            429 | 500..=599 => Disposition::Retryable,
            _ => Disposition::Terminal,
        }
    }

    fn stream_decoder(&self, ctx: StreamCtx<'_>) -> Option<Box<dyn StreamDecoder>> {
        let key = content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::ClaudeMessages,
        );
        crate::shared::claude::sse::ClaudeSseDecoder::for_operation(StreamCtx { key, ..ctx })
            .map(|decoder| Box::new(decoder) as Box<dyn StreamDecoder>)
    }

    fn extract_usage(&self, ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
        crate::shared::claude::usage::from_body(ctx.response_body)
    }

    fn prepare_quota_probe(
        &self,
        secret: &Value,
        provider_settings: &Value,
    ) -> Result<Option<http::Request<bytes::Bytes>>, ChannelError> {
        quota::probe_request(secret, provider_settings)
    }

    fn parse_quota_probe(
        &self,
        status: http::StatusCode,
        body: &[u8],
    ) -> Vec<gproxy_channel_api::QuotaObservation> {
        quota::parse_probe(status, body)
    }

    fn refresh_due(&self, secret: &Value) -> Option<i64> {
        auth::refresh_due(secret)
    }

    fn refresh<'a>(
        &'a self,
        secret: &'a Value,
        settings: &'a Value,
        http: &'a dyn SimpleHttp,
    ) -> Option<BoxFuture<'a, Result<Value, ChannelError>>> {
        Some(auth::refresh(secret, settings, http))
    }

    fn requires_continuations(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests;
