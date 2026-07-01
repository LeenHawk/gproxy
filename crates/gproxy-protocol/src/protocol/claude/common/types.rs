use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InferenceGeo(pub String);

macro_rules! extensible_string_enum {
    ($outer:ident, $known:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(untagged)]
        pub enum $outer {
            Known($known),
            Unknown(String),
        }

        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $known {
            $(
                #[serde(rename = $wire)]
                $variant,
            )+
        }
    };
}

extensible_string_enum!(ClaudeModel, ClaudeModelKnown {
    ClaudeFable5 => "claude-fable-5", ClaudeMythos5 => "claude-mythos-5",
    ClaudeOpus48 => "claude-opus-4-8", ClaudeOpus47 => "claude-opus-4-7",
    ClaudeMythosPreview => "claude-mythos-preview", ClaudeOpus46 => "claude-opus-4-6",
    ClaudeSonnet46 => "claude-sonnet-4-6", ClaudeHaiku45 => "claude-haiku-4-5",
    ClaudeHaiku4520251001 => "claude-haiku-4-5-20251001", ClaudeOpus45 => "claude-opus-4-5",
    ClaudeOpus4520251101 => "claude-opus-4-5-20251101", ClaudeSonnet45 => "claude-sonnet-4-5",
    ClaudeSonnet4520250929 => "claude-sonnet-4-5-20250929", ClaudeOpus41 => "claude-opus-4-1",
    ClaudeOpus4120250805 => "claude-opus-4-1-20250805", ClaudeOpus40 => "claude-opus-4-0",
    ClaudeOpus420250514 => "claude-opus-4-20250514", ClaudeSonnet40 => "claude-sonnet-4-0",
    ClaudeSonnet420250514 => "claude-sonnet-4-20250514", Claude3Haiku20240307 => "claude-3-haiku-20240307",
});

impl From<String> for ClaudeModel {
    fn from(value: String) -> Self {
        Self::Unknown(value)
    }
}

extensible_string_enum!(AnthropicBeta, AnthropicBetaKnown {
    MessageBatches20240924 => "message-batches-2024-09-24",
    PromptCaching20240731 => "prompt-caching-2024-07-31",
    ComputerUse20241022 => "computer-use-2024-10-22",
    ComputerUse20250124 => "computer-use-2025-01-24",
    Pdfs20240925 => "pdfs-2024-09-25",
    TokenCounting20241101 => "token-counting-2024-11-01",
    TokenEfficientTools20250219 => "token-efficient-tools-2025-02-19",
    Output128k20250219 => "output-128k-2025-02-19",
    FilesApi20250414 => "files-api-2025-04-14",
    McpClient20250404 => "mcp-client-2025-04-04",
    McpClient20251120 => "mcp-client-2025-11-20",
    DevFullThinking20250514 => "dev-full-thinking-2025-05-14",
    InterleavedThinking20250514 => "interleaved-thinking-2025-05-14",
    CodeExecution20250522 => "code-execution-2025-05-22",
    ExtendedCacheTtl20250411 => "extended-cache-ttl-2025-04-11",
    Context1m20250807 => "context-1m-2025-08-07",
    ContextManagement20250627 => "context-management-2025-06-27",
    ModelContextWindowExceeded20250826 => "model-context-window-exceeded-2025-08-26",
    Skills20251002 => "skills-2025-10-02",
    FastMode20260201 => "fast-mode-2026-02-01",
    Output300k20260324 => "output-300k-2026-03-24",
    UserProfiles20260324 => "user-profiles-2026-03-24",
    AdvisorTool20260301 => "advisor-tool-2026-03-01",
    ManagedAgents20260401 => "managed-agents-2026-04-01",
    CacheDiagnosis20260407 => "cache-diagnosis-2026-04-07",
    ThinkingTokenCount20260513 => "thinking-token-count-2026-05-13",
    ServerSideFallback20260601 => "server-side-fallback-2026-06-01",
    FallbackCredit20260601 => "fallback-credit-2026-06-01",
});

extensible_string_enum!(MessageRole, MessageRoleKnown { User => "user", Assistant => "assistant", System => "system" });
extensible_string_enum!(AssistantRole, AssistantRoleKnown { Assistant => "assistant" });
extensible_string_enum!(StopReason, StopReasonKnown {
    EndTurn => "end_turn", MaxTokens => "max_tokens", StopSequence => "stop_sequence",
    ToolUse => "tool_use", PauseTurn => "pause_turn", Compaction => "compaction",
    Refusal => "refusal", ModelContextWindowExceeded => "model_context_window_exceeded",
});
extensible_string_enum!(RequestServiceTier, RequestServiceTierKnown { Auto => "auto", StandardOnly => "standard_only" });
extensible_string_enum!(UsageServiceTier, UsageServiceTierKnown { Standard => "standard", Priority => "priority", Batch => "batch" });
extensible_string_enum!(Speed, SpeedKnown { Standard => "standard", Fast => "fast" });
extensible_string_enum!(CacheTtl, CacheTtlKnown { FiveMinutes => "5m", OneHour => "1h" });
extensible_string_enum!(ThinkingDisplay, ThinkingDisplayKnown { Summarized => "summarized", Omitted => "omitted" });
extensible_string_enum!(OutputEffort, OutputEffortKnown { Low => "low", Medium => "medium", High => "high", XHigh => "xhigh", Max => "max" });
extensible_string_enum!(ServerToolUseName, ServerToolUseNameKnown {
    Advisor => "advisor", WebSearch => "web_search", WebFetch => "web_fetch",
    CodeExecution => "code_execution", BashCodeExecution => "bash_code_execution",
    TextEditorCodeExecution => "text_editor_code_execution",
    ToolSearchToolRegex => "tool_search_tool_regex", ToolSearchToolBm25 => "tool_search_tool_bm25",
});
extensible_string_enum!(ToolType, ToolTypeKnown {
    Custom => "custom", Bash20241022 => "bash_20241022", Bash20250124 => "bash_20250124",
    CodeExecution20250522 => "code_execution_20250522", CodeExecution20250825 => "code_execution_20250825",
    CodeExecution20260120 => "code_execution_20260120", CodeExecution20260521 => "code_execution_20260521",
    Computer20241022 => "computer_20241022",
    Computer20250124 => "computer_20250124", Computer20251124 => "computer_20251124",
    Memory20250818 => "memory_20250818", TextEditor20241022 => "text_editor_20241022",
    TextEditor20250124 => "text_editor_20250124", TextEditor20250429 => "text_editor_20250429",
    TextEditor20250728 => "text_editor_20250728", WebSearch20250305 => "web_search_20250305",
    WebSearch20260209 => "web_search_20260209", WebFetch20250910 => "web_fetch_20250910",
    WebFetch20260209 => "web_fetch_20260209", WebFetch20260309 => "web_fetch_20260309",
    Advisor20260301 => "advisor_20260301", ToolSearchBm2520251119 => "tool_search_tool_bm25_20251119",
    ToolSearchBm25 => "tool_search_tool_bm25", ToolSearchRegex20251119 => "tool_search_tool_regex_20251119",
    ToolSearchRegex => "tool_search_tool_regex", McpToolset => "mcp_toolset",
});
extensible_string_enum!(CitationType, CitationTypeKnown {
    CharLocation => "char_location", PageLocation => "page_location",
    ContentBlockLocation => "content_block_location", WebSearchResultLocation => "web_search_result_location",
    SearchResultLocation => "search_result_location",
});
extensible_string_enum!(MessageObjectType, MessageObjectTypeKnown { Message => "message" });
extensible_string_enum!(ModelObjectType, ModelObjectTypeKnown { Model => "model" });
extensible_string_enum!(JsonSchemaObjectType, JsonSchemaObjectTypeKnown { Object => "object" });
extensible_string_enum!(JsonSchemaFormatType, JsonSchemaFormatTypeKnown { JsonSchema => "json_schema" });
extensible_string_enum!(McpServerType, McpServerTypeKnown { Url => "url" });
extensible_string_enum!(TaskBudgetType, TaskBudgetTypeKnown { Tokens => "tokens" });
extensible_string_enum!(SkillType, SkillTypeKnown { Anthropic => "anthropic", Custom => "custom" });
extensible_string_enum!(IterationUsageType, IterationUsageTypeKnown { Message => "message", Compaction => "compaction", AdvisorMessage => "advisor_message", FallbackMessage => "fallback_message" });
extensible_string_enum!(ContextEditType, ContextEditTypeKnown {
    ClearToolUses20250919 => "clear_tool_uses_20250919",
    ClearThinking20251015 => "clear_thinking_20251015",
    Compact20260112 => "compact_20260112",
});
