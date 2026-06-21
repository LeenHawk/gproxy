use serde::{Deserialize, Serialize};

strict_string_enum!(EmbeddingEncodingFormat {
    Float => "float",
    Base64 => "base64",
});

strict_string_enum!(ReasoningEffort {
    None => "none",
    Minimal => "minimal",
    Low => "low",
    Medium => "medium",
    High => "high",
    XHigh => "xhigh",
});

strict_string_enum!(ReasoningSummary {
    Auto => "auto",
    Concise => "concise",
    Detailed => "detailed",
});

strict_string_enum!(ServiceTier {
    Auto => "auto",
    Default => "default",
    Flex => "flex",
    Scale => "scale",
    Priority => "priority",
    OnDemand => "on_demand",
});

strict_string_enum!(TruncationStrategy {
    Auto => "auto",
    Disabled => "disabled",
});

strict_string_enum!(Verbosity {
    Low => "low",
    Medium => "medium",
    High => "high",
});

strict_string_enum!(PromptCacheRetention {
    InMemory => "in_memory",
    TwentyFourHours => "24h",
});

extensible_string_enum!(ResponsePersonality, ResponsePersonalityKnown {
    Friendly => "friendly",
    Pragmatic => "pragmatic",
});

extensible_string_enum!(ContextManagementType, ContextManagementTypeKnown {
    Compaction => "compaction",
});

strict_string_enum!(SearchContextSize {
    Low => "low",
    Medium => "medium",
    High => "high",
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApproximateLocationType {
    #[serde(rename = "approximate")]
    Approximate,
}

strict_string_enum!(TextOrAudioModality {
    Text => "text",
    Audio => "audio",
});

strict_string_enum!(InputAudioFormat {
    Wav => "wav",
    Mp3 => "mp3",
});

strict_string_enum!(AudioResponseFormat {
    Wav => "wav",
    Aac => "aac",
    Mp3 => "mp3",
    Flac => "flac",
    Opus => "opus",
    Pcm16 => "pcm16",
});

extensible_string_enum!(VoiceName, VoiceNameKnown {
    Alloy => "alloy",
    Ash => "ash",
    Ballad => "ballad",
    Coral => "coral",
    Echo => "echo",
    Fable => "fable",
    Nova => "nova",
    Onyx => "onyx",
    Sage => "sage",
    Shimmer => "shimmer",
    Verse => "verse",
    Marin => "marin",
    Cedar => "cedar",
});

strict_string_enum!(DetailLevel {
    Auto => "auto",
    Low => "low",
    High => "high",
    Original => "original",
});

strict_string_enum!(ChatImageDetailLevel {
    Auto => "auto",
    Low => "low",
    High => "high",
});

strict_string_enum!(InputFileDetailLevel {
    Low => "low",
    High => "high",
});
