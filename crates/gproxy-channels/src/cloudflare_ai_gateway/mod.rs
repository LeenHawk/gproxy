mod routes;

mod model;
mod prepare;
mod sse;
mod usage;

use gproxy_channel_api::{
    Channel, ChannelDescriptor, ChannelSupport, Disposition, NormalizedUsage, PrepareCtx,
    PreparedRequest, ResponseView, StreamCtx, StreamDecoder, UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey};

pub struct CloudflareAiGatewayChannel;

const fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

static SUPPORTS: [ChannelSupport; 8] = [
    ChannelSupport::passthrough(content(
        Operation::GenerateContent,
        ContentGenerationKind::OpenAiChat,
    )),
    ChannelSupport::passthrough(content(
        Operation::GenerateContent,
        ContentGenerationKind::OpenAiResponses,
    )),
    ChannelSupport::passthrough(content(
        Operation::GenerateContent,
        ContentGenerationKind::ClaudeMessages,
    )),
    ChannelSupport::passthrough(content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiChat,
    )),
    ChannelSupport::passthrough(content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiResponses,
    )),
    ChannelSupport::passthrough(content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::ClaudeMessages,
    )),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::GeminiGenerateContent,
        ),
        content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::GeminiGenerateContent,
        ),
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
    ),
];

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "cloudflare-ai-gateway",
    display_name: "Cloudflare AI Gateway",
    supports: &SUPPORTS,
    provider_fields: crate::metadata::BASE_URL,
    credential_fields: crate::metadata::API_KEY,
    endpoint_overrides: true,
    traffic_policy: crate::policy::CLOUDFLARE,
};

impl Channel for CloudflareAiGatewayChannel {
    fn routing_table(&self) -> &'static [ChannelSupport] {
        routes::ROUTES
    }

    fn descriptor(&self) -> &'static ChannelDescriptor {
        &DESCRIPTOR
    }

    fn prepare(
        &self,
        ctx: PrepareCtx<'_>,
    ) -> Result<PreparedRequest, gproxy_channel_api::ChannelError> {
        prepare::request(ctx)
    }

    fn classify(&self, response: ResponseView<'_>) -> Disposition {
        match response.status.as_u16() {
            200..=299 => Disposition::Success,
            401 => Disposition::CredentialDead,
            429 | 500..=599 => Disposition::Retryable,
            _ => Disposition::Terminal,
        }
    }

    fn stream_decoder(&self, ctx: StreamCtx<'_>) -> Option<Box<dyn StreamDecoder>> {
        sse::decoder(ctx)
    }

    fn extract_usage(&self, ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
        usage::from_body(ctx)
    }
}

#[cfg(test)]
mod tests;
