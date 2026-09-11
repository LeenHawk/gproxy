use gproxy_channel_api::ChannelSupport;
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind};

use super::auth::Mode;
use super::routes;

/// Kimi merges two credential families behind one channel: OAuth credentials
/// reach the native Claude wire, API keys only speak chat. Both rows are
/// declared for the same source in [`routes::ROUTES`]; the credential shape
/// decides which one this request takes.
pub(super) fn support(source: OperationKey, mode: Mode) -> Option<ChannelSupport> {
    if mode == Mode::ApiKey && source.operation() == Operation::CountTokens {
        return None;
    }
    let mut rows = routes::ROUTES.iter().filter(|row| row.source == source);
    match mode {
        Mode::Oauth => rows
            .clone()
            .find(|row| row.source == row.target)
            .or_else(|| rows.next()),
        Mode::ApiKey => rows.find(|row| {
            !matches!(
                source.kind(),
                OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
            ) || row.target.kind()
                == OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat)
        }),
    }
    .copied()
}
