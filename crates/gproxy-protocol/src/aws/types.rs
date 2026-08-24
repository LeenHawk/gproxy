use serde::{Deserialize, Serialize};

pub type Rest = serde_json::Map<String, serde_json::Value>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EmptyObject {
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

macro_rules! extensible_string_enum {
    ($outer:ident, $known:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(untagged)]
        #[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
        pub enum $outer {
            Known($known),
            Unknown(String),
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
        pub enum $known {
            $(#[serde(rename = $wire)] $variant),+
        }
    };
}

extensible_string_enum!(ConversationRole, ConversationRoleKnown {
    User => "user", Assistant => "assistant", System => "system",
});
extensible_string_enum!(StopReason, StopReasonKnown {
    EndTurn => "end_turn", ToolUse => "tool_use", MaxTokens => "max_tokens",
    StopSequence => "stop_sequence", GuardrailIntervened => "guardrail_intervened",
    ContentFiltered => "content_filtered", MalformedModelOutput => "malformed_model_output",
    MalformedToolUse => "malformed_tool_use",
    ModelContextWindowExceeded => "model_context_window_exceeded",
});
extensible_string_enum!(ServiceTierType, ServiceTierTypeKnown {
    Priority => "priority", Default => "default", Flex => "flex", Reserved => "reserved",
});
extensible_string_enum!(PerformanceLatency, PerformanceLatencyKnown {
    Standard => "standard", Optimized => "optimized",
});
extensible_string_enum!(CachePointType, CachePointTypeKnown { Default => "default" });
extensible_string_enum!(CacheTtl, CacheTtlKnown { FiveMinutes => "5m", OneHour => "1h" });
extensible_string_enum!(ToolResultStatus, ToolResultStatusKnown {
    Success => "success", Error => "error",
});
extensible_string_enum!(ToolUseType, ToolUseTypeKnown {
    ServerToolUse => "server_tool_use",
});
extensible_string_enum!(GuardrailTrace, GuardrailTraceKnown {
    Enabled => "enabled", Disabled => "disabled", EnabledFull => "enabled_full",
});
extensible_string_enum!(GuardrailStreamProcessingMode, GuardrailStreamProcessingModeKnown {
    Sync => "sync", Async => "async",
});
extensible_string_enum!(OutputFormatType, OutputFormatTypeKnown {
    JsonSchema => "json_schema",
});
extensible_string_enum!(ImageFormat, ImageFormatKnown {
    Png => "png", Jpeg => "jpeg", Gif => "gif", Webp => "webp",
});
extensible_string_enum!(DocumentFormat, DocumentFormatKnown {
    Pdf => "pdf", Csv => "csv", Doc => "doc", Docx => "docx", Xls => "xls",
    Xlsx => "xlsx", Html => "html", Txt => "txt", Md => "md",
});
extensible_string_enum!(AsyncInvokeStatus, AsyncInvokeStatusKnown {
    InProgress => "InProgress", Completed => "Completed", Failed => "Failed",
});
extensible_string_enum!(CustomizationType, CustomizationTypeKnown {
    FineTuning => "FINE_TUNING", ContinuedPreTraining => "CONTINUED_PRE_TRAINING",
    Distillation => "DISTILLATION",
});
extensible_string_enum!(InferenceType, InferenceTypeKnown {
    OnDemand => "ON_DEMAND", Provisioned => "PROVISIONED",
});
extensible_string_enum!(ModelModality, ModelModalityKnown {
    Text => "TEXT", Image => "IMAGE", Embedding => "EMBEDDING",
});
extensible_string_enum!(FoundationModelLifecycleStatus, FoundationModelLifecycleStatusKnown {
    Active => "ACTIVE", Legacy => "LEGACY",
});
