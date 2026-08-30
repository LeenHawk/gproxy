mod routes;

mod model;
mod prepare;
mod shape;
mod sse;
mod usage;

use gproxy_channel_api::{
    Channel, ChannelDescriptor, ChannelSupport, Disposition, NormalizedUsage, PrepareCtx,
    PreparedRequest, ResponseView, StreamCtx, StreamDecoder, UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};

pub struct ClaudeApiChannel;

const fn family(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::Claude)
}

const fn content(operation: Operation) -> OperationKey {
    OperationKey::content(operation, ContentGenerationKind::ClaudeMessages)
}

static SUPPORTS: [ChannelSupport; 15] = [
    ChannelSupport::passthrough(family(Operation::ListModels)),
    ChannelSupport::passthrough(family(Operation::GetModel)),
    ChannelSupport::passthrough(family(Operation::CountTokens)),
    ChannelSupport::passthrough(content(Operation::GenerateContent)),
    ChannelSupport::passthrough(content(Operation::StreamGenerateContent)),
    ChannelSupport::transform(
        OperationKey::family(Operation::ListModels, WireFamily::OpenAi),
        family(Operation::ListModels),
    ),
    ChannelSupport::transform(
        OperationKey::family(Operation::GetModel, WireFamily::OpenAi),
        family(Operation::GetModel),
    ),
    ChannelSupport::transform(
        OperationKey::family(Operation::CountTokens, WireFamily::OpenAi),
        family(Operation::CountTokens),
    ),
    ChannelSupport::transform(
        OperationKey::content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
        content(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        OperationKey::content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
        content(Operation::StreamGenerateContent),
    ),
    ChannelSupport::transform(
        OperationKey::content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        content(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        OperationKey::content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        content(Operation::StreamGenerateContent),
    ),
    ChannelSupport::transform(
        OperationKey::family(Operation::CompactContent, WireFamily::OpenAi),
        content(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        OperationKey::content(
            Operation::GenerateContent,
            ContentGenerationKind::GeminiGenerateContent,
        ),
        content(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        OperationKey::content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::GeminiGenerateContent,
        ),
        content(Operation::StreamGenerateContent),
    ),
];

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "claudeapi",
    display_name: "Claude API",
    supports: &SUPPORTS,
    provider_fields: crate::metadata::CLAUDE,
    credential_fields: crate::metadata::API_KEY,
    endpoint_overrides: true,
};

impl Channel for ClaudeApiChannel {
    fn routing_table(&self) -> &'static [ChannelSupport] {
        routes::ROUTES
    }

    fn descriptor(&self) -> &'static ChannelDescriptor {
        &DESCRIPTOR
    }

    fn default_rule_set(&self) -> Option<gproxy_channel_api::ChannelDefaultRuleSet> {
        Some(gproxy_channel_api::ChannelDefaultRuleSet {
            id: "system-cache",
            name: "gproxy:channel-default:claudeapi:system-cache",
            description: "gproxy:channel-default:claudeapi:system-cache",
            rules: vec![gproxy_channel_api::ChannelDefaultRule {
                kind: "cache_breakpoint",
                config: serde_json::json!({"target":"system","index":null,"ttl":"1h"}),
                filter_operations: Some(vec![
                    "generate_content".into(),
                    "stream_generate_content".into(),
                ]),
                sort_order: 0,
            }],
        })
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
        sse::ClaudeSseDecoder::for_operation(ctx)
            .map(|decoder| Box::new(decoder) as Box<dyn StreamDecoder>)
    }

    fn extract_usage(&self, ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
        usage::from_body(ctx.response_body)
    }
}

#[cfg(test)]
mod tests;
