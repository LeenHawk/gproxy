use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAiModelId {
    Known(OpenAiModelIdKnown),
    Unknown(String),
}

impl From<String> for OpenAiModelId {
    fn from(value: String) -> Self {
        Self::Unknown(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpenAiModelIdKnown {
    #[serde(rename = "gpt-5.5")]
    Gpt55,
    #[serde(rename = "gpt-5.4")]
    Gpt54,
    #[serde(rename = "gpt-5.4-mini")]
    Gpt54Mini,
    #[serde(rename = "gpt-5.4-nano")]
    Gpt54Nano,
    #[serde(rename = "gpt-5.4-mini-2026-03-17")]
    Gpt54Mini20260317,
    #[serde(rename = "gpt-5.4-nano-2026-03-17")]
    Gpt54Nano20260317,
    #[serde(rename = "gpt-5.3-codex-spark")]
    Gpt53CodexSpark,
    #[serde(rename = "gpt-5.3-chat-latest")]
    Gpt53ChatLatest,
    #[serde(rename = "gpt-5.2")]
    Gpt52,
    #[serde(rename = "gpt-5.2-2025-12-11")]
    Gpt5220251211,
    #[serde(rename = "gpt-5.2-chat-latest")]
    Gpt52ChatLatest,
    #[serde(rename = "gpt-5.2-pro")]
    Gpt52Pro,
    #[serde(rename = "gpt-5.2-pro-2025-12-11")]
    Gpt52Pro20251211,
    #[serde(rename = "gpt-5.1")]
    Gpt51,
    #[serde(rename = "gpt-5.1-2025-11-13")]
    Gpt5120251113,
    #[serde(rename = "gpt-5.1-codex")]
    Gpt51Codex,
    #[serde(rename = "gpt-5.1-mini")]
    Gpt51Mini,
    #[serde(rename = "gpt-5.1-chat-latest")]
    Gpt51ChatLatest,
    #[serde(rename = "gpt-5")]
    Gpt5,
    #[serde(rename = "gpt-5-mini")]
    Gpt5Mini,
    #[serde(rename = "gpt-5-nano")]
    Gpt5Nano,
    #[serde(rename = "gpt-5-2025-08-07")]
    Gpt520250807,
    #[serde(rename = "gpt-5-mini-2025-08-07")]
    Gpt5Mini20250807,
    #[serde(rename = "gpt-5-nano-2025-08-07")]
    Gpt5Nano20250807,
    #[serde(rename = "gpt-5-chat-latest")]
    Gpt5ChatLatest,
    #[serde(rename = "gpt-4.1")]
    Gpt41,
    #[serde(rename = "gpt-4.1-mini")]
    Gpt41Mini,
    #[serde(rename = "gpt-4.1-nano")]
    Gpt41Nano,
    #[serde(rename = "gpt-4.1-2025-04-14")]
    Gpt4120250414,
    #[serde(rename = "gpt-4.1-mini-2025-04-14")]
    Gpt41Mini20250414,
    #[serde(rename = "gpt-4.1-nano-2025-04-14")]
    Gpt41Nano20250414,
    #[serde(rename = "o4-mini")]
    O4Mini,
    #[serde(rename = "o4-mini-2025-04-16")]
    O4Mini20250416,
    #[serde(rename = "o3")]
    O3,
    #[serde(rename = "o3-2025-04-16")]
    O320250416,
    #[serde(rename = "o3-mini")]
    O3Mini,
    #[serde(rename = "o3-mini-2025-01-31")]
    O3Mini20250131,
    #[serde(rename = "o1")]
    O1,
    #[serde(rename = "o1-2024-12-17")]
    O120241217,
    #[serde(rename = "o1-preview")]
    O1Preview,
    #[serde(rename = "o1-preview-2024-09-12")]
    O1Preview20240912,
    #[serde(rename = "o1-mini")]
    O1Mini,
    #[serde(rename = "o1-mini-2024-09-12")]
    O1Mini20240912,
    #[serde(rename = "gpt-4o")]
    Gpt4o,
    #[serde(rename = "gpt-4o-2024-11-20")]
    Gpt4o20241120,
    #[serde(rename = "gpt-4o-2024-08-06")]
    Gpt4o20240806,
    #[serde(rename = "gpt-4o-2024-05-13")]
    Gpt4o20240513,
    #[serde(rename = "gpt-4o-audio-preview")]
    Gpt4oAudioPreview,
    #[serde(rename = "gpt-4o-audio-preview-2024-10-01")]
    Gpt4oAudioPreview20241001,
    #[serde(rename = "gpt-4o-audio-preview-2024-12-17")]
    Gpt4oAudioPreview20241217,
    #[serde(rename = "gpt-4o-audio-preview-2025-06-03")]
    Gpt4oAudioPreview20250603,
    #[serde(rename = "gpt-4o-mini-audio-preview")]
    Gpt4oMiniAudioPreview,
    #[serde(rename = "gpt-4o-mini-audio-preview-2024-12-17")]
    Gpt4oMiniAudioPreview20241217,
    #[serde(rename = "gpt-4o-search-preview")]
    Gpt4oSearchPreview,
    #[serde(rename = "gpt-4o-mini-search-preview")]
    Gpt4oMiniSearchPreview,
    #[serde(rename = "gpt-4o-search-preview-2025-03-11")]
    Gpt4oSearchPreview20250311,
    #[serde(rename = "gpt-4o-mini-search-preview-2025-03-11")]
    Gpt4oMiniSearchPreview20250311,
    #[serde(rename = "chatgpt-4o-latest")]
    ChatGpt4oLatest,
    #[serde(rename = "codex-mini-latest")]
    CodexMiniLatest,
    #[serde(rename = "gpt-4o-mini")]
    Gpt4oMini,
    #[serde(rename = "gpt-4o-mini-2024-07-18")]
    Gpt4oMini20240718,
    #[serde(rename = "gpt-4-turbo")]
    Gpt4Turbo,
    #[serde(rename = "gpt-4-turbo-2024-04-09")]
    Gpt4Turbo20240409,
    #[serde(rename = "gpt-4-0125-preview")]
    Gpt40125Preview,
    #[serde(rename = "gpt-4-turbo-preview")]
    Gpt4TurboPreview,
    #[serde(rename = "gpt-4-1106-preview")]
    Gpt41106Preview,
    #[serde(rename = "gpt-4-vision-preview")]
    Gpt4VisionPreview,
    #[serde(rename = "gpt-4")]
    Gpt4,
    #[serde(rename = "gpt-4-0314")]
    Gpt40314,
    #[serde(rename = "gpt-4-0613")]
    Gpt40613,
    #[serde(rename = "gpt-4-32k")]
    Gpt432k,
    #[serde(rename = "gpt-4-32k-0314")]
    Gpt432k0314,
    #[serde(rename = "gpt-4-32k-0613")]
    Gpt432k0613,
    #[serde(rename = "gpt-3.5-turbo")]
    Gpt35Turbo,
    #[serde(rename = "gpt-3.5-turbo-16k")]
    Gpt35Turbo16k,
    #[serde(rename = "gpt-3.5-turbo-0301")]
    Gpt35Turbo0301,
    #[serde(rename = "gpt-3.5-turbo-0613")]
    Gpt35Turbo0613,
    #[serde(rename = "gpt-3.5-turbo-1106")]
    Gpt35Turbo1106,
    #[serde(rename = "gpt-3.5-turbo-0125")]
    Gpt35Turbo0125,
    #[serde(rename = "gpt-3.5-turbo-16k-0613")]
    Gpt35Turbo16k0613,
    #[serde(rename = "o1-pro")]
    O1Pro,
    #[serde(rename = "o1-pro-2025-03-19")]
    O1Pro20250319,
    #[serde(rename = "o3-pro")]
    O3Pro,
    #[serde(rename = "o3-pro-2025-06-10")]
    O3Pro20250610,
    #[serde(rename = "o3-deep-research")]
    O3DeepResearch,
    #[serde(rename = "o3-deep-research-2025-06-26")]
    O3DeepResearch20250626,
    #[serde(rename = "o4-mini-deep-research")]
    O4MiniDeepResearch,
    #[serde(rename = "o4-mini-deep-research-2025-06-26")]
    O4MiniDeepResearch20250626,
    #[serde(rename = "computer-use-preview")]
    ComputerUsePreview,
    #[serde(rename = "computer-use-preview-2025-03-11")]
    ComputerUsePreview20250311,
    #[serde(rename = "gpt-5-codex")]
    Gpt5Codex,
    #[serde(rename = "gpt-5-pro")]
    Gpt5Pro,
    #[serde(rename = "gpt-5-pro-2025-10-06")]
    Gpt5Pro20251006,
    #[serde(rename = "gpt-5.1-codex-max")]
    Gpt51CodexMax,
    #[serde(rename = "text-embedding-ada-002")]
    TextEmbeddingAda002,
    #[serde(rename = "text-embedding-3-small")]
    TextEmbedding3Small,
    #[serde(rename = "text-embedding-3-large")]
    TextEmbedding3Large,
    #[serde(rename = "gpt-image-1.5")]
    GptImage15,
    #[serde(rename = "dall-e-2")]
    DallE2,
    #[serde(rename = "dall-e-3")]
    DallE3,
    #[serde(rename = "gpt-image-1")]
    GptImage1,
    #[serde(rename = "gpt-image-1-mini")]
    GptImage1Mini,
    #[serde(rename = "chatgpt-image-latest")]
    ChatGptImageLatest,
    #[serde(rename = "omni-moderation-latest")]
    OmniModerationLatest,
}
