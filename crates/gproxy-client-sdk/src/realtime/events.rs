use super::*;

/// Native event payloads are observational data, including unknown extensions.
/// Local events have no native payload. The collector never archives raw frames.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RealtimeMetadata {
    pub vendor: Vendor,
    pub native: Option<Value>,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RealtimeItem {
    pub id: String,
    pub content_index: i64,
}
impl RealtimeItem {
    pub(crate) fn new(id: impl Into<String>, content_index: i64) -> Self {
        Self {
            id: id.into(),
            content_index,
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RealtimeUsage {
    pub tokens: Usage,
    pub total_tokens: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RealtimeEvent {
    pub kind: RealtimeEventKind,
    pub metadata: RealtimeMetadata,
}
impl RealtimeEvent {
    pub(crate) fn local(vendor: Vendor, kind: RealtimeEventKind) -> Self {
        Self {
            kind,
            metadata: RealtimeMetadata {
                vendor,
                native: None,
            },
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum RealtimeEventKind {
    Ready,
    Updated,
    SpeechStarted {
        item: RealtimeItem,
        audio_ms: Option<u64>,
    },
    SpeechStopped {
        item: RealtimeItem,
        audio_ms: Option<u64>,
    },
    InputCommitted {
        item: RealtimeItem,
    },
    InputCleared,
    InputTranscriptDelta {
        item: RealtimeItem,
        delta: String,
    },
    InputTranscriptDone {
        item: RealtimeItem,
        text: String,
    },
    InputTranscriptPreview {
        item: RealtimeItem,
        text: String,
    },
    InputTranscriptionFailed {
        item: RealtimeItem,
        error: Error,
    },
    ResponseStarted {
        response_id: Option<String>,
    },
    AudioDelta {
        item: RealtimeItem,
        bytes: Vec<u8>,
        format: AudioFormat,
    },
    OutputTranscriptDelta {
        item: RealtimeItem,
        delta: String,
    },
    OutputTranscriptDone {
        item: RealtimeItem,
        text: String,
    },
    TextDelta {
        item: RealtimeItem,
        delta: String,
    },
    TextDone {
        item: RealtimeItem,
        text: String,
    },
    InputText {
        item: RealtimeItem,
        text: String,
    },
    ToolCall {
        item: RealtimeItem,
        call: crate::ToolCall,
    },
    ToolCallCancelled {
        ids: Vec<String>,
    },
    ToolResultSent {
        result: crate::ToolResult,
    },
    /// Playback belongs to the caller; no server output-buffer event is sent.
    ClearLocalPlayback,
    Interrupted {
        item: RealtimeItem,
        played_ms: Option<u64>,
    },
    ResponseDone {
        response_id: Option<String>,
        finish_reason: FinishReason,
        usage: RealtimeUsage,
    },
    RateLimits {
        #[cfg_attr(feature = "ts", ts(as = "Value"))]
        limits: Value,
    },
    GoAway {
        time_left: Option<String>,
    },
    Resumption {
        handle: Option<String>,
        resumable: Option<bool>,
    },
    Warning {
        warning: crate::Warning,
    },
    Error {
        error: crate::Error,
        fatal: bool,
    },
    Closed {
        code: Option<u16>,
        reason: String,
    },
    /// A recognized vendor event with no portable meaning (lifecycle and
    /// bookkeeping frames). The payload is in `RealtimeMetadata::native`.
    Native {
        event: String,
    },
    /// A frame the SDK does not recognize.
    #[serde(untagged)]
    Unknown(Value),
}
