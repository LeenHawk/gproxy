pub use crate::transform::Converted;
use crate::transform::TransformError;
use crate::wire::claude::models as claude_models;
use serde::{Deserialize, Serialize};

/// Facts needed by targets whose model schema requires an owner or creation
/// timestamp absent from the source. A source fact always wins; a conflicting
/// supplied fact is rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenAiModelSupplement {
    pub owned_by: String,
    pub created: Option<i64>,
}

/// Target-specific Claude metadata. The nested capability facts are rebuilt
/// into fresh wire structs so no supplement or source `rest` map is copied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeModelSupplement {
    pub display_name: Option<String>,
    #[serde(default)]
    pub line: Option<String>,
    pub allowed_fallback_models: Vec<String>,
    pub capabilities: ClaudeCapabilities,
    pub max_input_tokens: Option<i64>,
    pub max_tokens: Option<i64>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeCapabilities {
    pub batch: bool,
    pub citations: bool,
    pub code_execution: bool,
    pub context_management: ClaudeContextManagement,
    pub effort: ClaudeEffort,
    pub image_input: bool,
    pub pdf_input: bool,
    pub structured_outputs: bool,
    #[serde(default)]
    pub server_tools: Option<ClaudeServerTools>,
    pub thinking: ClaudeThinking,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeContextManagement {
    pub clear_thinking_20251015: bool,
    pub clear_tool_uses_20250919: bool,
    pub compact_20260112: bool,
    pub supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeEffort {
    pub high: bool,
    pub low: bool,
    pub max: bool,
    pub medium: bool,
    pub supported: bool,
    pub xhigh: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeThinking {
    pub supported: bool,
    pub adaptive: bool,
    pub enabled: bool,
    #[serde(default)]
    pub disabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeServerTools {
    pub supported: bool,
    pub web_search: bool,
    pub code_execution: bool,
}

impl ClaudeCapabilities {
    pub(crate) fn into_wire(self) -> claude_models::ModelCapabilities {
        let support = |supported| claude_models::CapabilitySupport {
            supported,
            rest: Default::default(),
        };
        claude_models::ModelCapabilities {
            batch: support(self.batch),
            citations: support(self.citations),
            code_execution: support(self.code_execution),
            context_management: claude_models::ContextManagementCapability {
                clear_thinking_20251015: support(self.context_management.clear_thinking_20251015),
                clear_tool_uses_20250919: support(self.context_management.clear_tool_uses_20250919),
                compact_20260112: support(self.context_management.compact_20260112),
                supported: self.context_management.supported,
                rest: Default::default(),
            },
            effort: claude_models::EffortCapability {
                high: support(self.effort.high),
                low: support(self.effort.low),
                max: support(self.effort.max),
                medium: support(self.effort.medium),
                supported: self.effort.supported,
                xhigh: support(self.effort.xhigh),
                rest: Default::default(),
            },
            image_input: support(self.image_input),
            pdf_input: support(self.pdf_input),
            structured_outputs: support(self.structured_outputs),
            server_tools: self
                .server_tools
                .map(|tools| claude_models::ServerToolsCapability {
                    supported: tools.supported,
                    web_search: support(tools.web_search),
                    code_execution: support(tools.code_execution),
                    rest: Default::default(),
                }),
            thinking: claude_models::ThinkingCapability {
                supported: self.thinking.supported,
                types: claude_models::ThinkingTypes {
                    adaptive: support(self.thinking.adaptive),
                    enabled: support(self.thinking.enabled),
                    disabled: self.thinking.disabled.map(support),
                    rest: Default::default(),
                },
                rest: Default::default(),
            },
            rest: Default::default(),
        }
    }
}

/// Facts for Gemini's required `baseModelId` and `version` fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeminiModelSupplement {
    pub base_model_id: String,
    pub version: String,
}

/// Pagination facts are target-specific. A cursor is never renamed to a page
/// token implicitly. `complete` means the caller has aggregated the complete
/// directory from the first request through its terminal page; it does not
/// mean merely that the current source page has `has_more = false`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListPageFacts {
    pub complete: bool,
    pub first_id: Option<String>,
    pub last_id: Option<String>,
    pub has_more: Option<bool>,
    pub next_page_token: Option<String>,
}

impl ListPageFacts {
    pub const fn complete() -> Self {
        Self {
            complete: true,
            first_id: None,
            last_id: None,
            has_more: Some(false),
            next_page_token: None,
        }
    }

    pub fn explicit(
        first_id: impl Into<String>,
        last_id: impl Into<String>,
        has_more: bool,
    ) -> Self {
        Self {
            complete: false,
            first_id: Some(first_id.into()),
            last_id: Some(last_id.into()),
            has_more: Some(has_more),
            next_page_token: None,
        }
    }

    pub fn with_next_page_token(mut self, token: impl Into<String>) -> Self {
        self.next_page_token = Some(token.into());
        self
    }
}

pub(crate) fn missing_pagination(detail: &str) -> TransformError {
    TransformError::new(
        crate::transform::TransformErrorKind::MissingMetadata,
        "pagination",
        detail,
    )
}
