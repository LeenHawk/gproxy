use gproxy_channel_api::ChannelSupport;
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};

use super::auth::Mode;

const fn family(operation: Operation, family: WireFamily) -> OperationKey {
    OperationKey::family(operation, family)
}

const fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

const fn chat(operation: Operation) -> OperationKey {
    content(operation, ContentGenerationKind::OpenAiChat)
}

pub(super) static SUPPORTS: [ChannelSupport; 17] = [
    ChannelSupport::passthrough(family(Operation::ListModels, WireFamily::OpenAi)),
    ChannelSupport::transform(
        family(Operation::ListModels, WireFamily::Claude),
        family(Operation::ListModels, WireFamily::OpenAi),
    ),
    ChannelSupport::local(family(Operation::CountTokens, WireFamily::Claude)),
    ChannelSupport::local(family(Operation::CountTokens, WireFamily::OpenAi)),
    ChannelSupport::passthrough(chat(Operation::GenerateContent)),
    ChannelSupport::passthrough(chat(Operation::StreamGenerateContent)),
    ChannelSupport::passthrough(content(
        Operation::GenerateContent,
        ContentGenerationKind::OpenAiResponses,
    )),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        chat(Operation::GenerateContent),
    ),
    ChannelSupport::passthrough(content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiResponses,
    )),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        chat(Operation::StreamGenerateContent),
    ),
    ChannelSupport::passthrough(content(
        Operation::GenerateContent,
        ContentGenerationKind::ClaudeMessages,
    )),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        chat(Operation::GenerateContent),
    ),
    ChannelSupport::passthrough(content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::ClaudeMessages,
    )),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        chat(Operation::StreamGenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::GeminiGenerateContent,
        ),
        chat(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::GeminiGenerateContent,
        ),
        chat(Operation::StreamGenerateContent),
    ),
    ChannelSupport::passthrough(family(Operation::CreateEmbedding, WireFamily::OpenAi)),
];

pub(super) fn select(source: OperationKey, mode: Mode) -> Option<ChannelSupport> {
    if mode == Mode::ApiKey
        && matches!(
            source.operation,
            Operation::CountTokens | Operation::CreateEmbedding
        )
    {
        return None;
    }
    let mut rows = SUPPORTS.iter().filter(|row| row.source == source);
    match mode {
        Mode::Oauth => rows
            .clone()
            .find(|row| row.source == row.target)
            .or_else(|| rows.next()),
        Mode::ApiKey => rows.find(|row| {
            !matches!(
                source.kind,
                gproxy_protocol::OperationKind::ContentGeneration(
                    ContentGenerationKind::OpenAiResponses | ContentGenerationKind::ClaudeMessages
                )
            ) || row.target.kind
                == gproxy_protocol::OperationKind::ContentGeneration(
                    ContentGenerationKind::OpenAiChat,
                )
        }),
    }
    .copied()
}
