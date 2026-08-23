mod dispatch;
mod resolve;

pub(crate) use dispatch::{request, response};
pub(crate) use resolve::resolve;

#[derive(Clone, Copy)]
pub(crate) enum TransformPair {
    OpenAiModelsToClaude,
    ClaudeModelsToOpenAi,
    OpenAiCountToClaude,
    ClaudeCountToOpenAi,
    ChatToClaude,
    ClaudeToChat,
    ResponsesToClaude,
    ClaudeToResponses,
    ClaudeToGemini,
    GeminiToClaude,
    GeminiToChat,
    ChatToGemini,
    GeminiToResponses,
    ResponsesToGemini,
    OpenAiChatToResponses,
    OpenAiResponsesToChat,
    CompactToClaude,
}
