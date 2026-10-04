use serde::{Deserialize, Serialize};

use crate::Rest;

/// Query/path request shapes for `GET /models` and `GET /models/{model}`.
/// Source: `upstream_docs/openai/docs/List models.md` and `Retrieve model.md`.
/// Extended catalog fields follow `samples/codex/codex-rs/protocol/src/openai_models.rs`.
pub type ListModelsRequest = crate::WireRequest<()>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ListObject {
    #[default]
    List,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ModelObject {
    Model,
}

pub type GetModelRequest = crate::WireRequest<()>;

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ListModelsResponseBody {
    #[serde(default)]
    pub data: Vec<Model>,
    #[serde(default)]
    pub object: ListObject,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<Model>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

pub type ListModelsResponse = crate::WireResponse<ListModelsResponseBody>;
pub type GetModelResponse = crate::WireResponse<Model>;

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct Model {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    /// Some compatible catalogs do not report a creation timestamp.
    #[wire(required)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<i64>,
    #[wire(required)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<ModelObject>,
    #[wire(required)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owned_by: Option<String>,

    // GPROXY model extensions, matching the v3 OpenAI model contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_context_window: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_supported: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_modalities: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_modalities: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supported_parameters: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "reasoning_levels",
        skip_serializing_if = "Option::is_none"
    )]
    pub supported_reasoning_levels: Option<Vec<ModelReasoningLevel>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_reasoning_level: Option<ModelReasoningEffort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tiers: Option<Vec<ModelServiceTier>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_service_tier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_methods: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supported_actions: Option<Vec<String>>,

    // Codex model catalog extensions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guardian: Option<GuardianModelPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_type: Option<ConfigShellToolType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<ModelVisibility>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supported_in_api: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_speed_tiers: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_access_programs: Option<ModelAccessPrograms>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub availability_nux: Option<ModelAvailabilityNux>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upgrade: Option<ModelInfoUpgrade>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_messages: Option<ModelMessages>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_skills_usage_instructions: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_plugin_usage_instructions: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_apps_usage_instructions: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_reasoning_summary_parameter: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_reasoning_summary: Option<ModelReasoningSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_verbosity: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_verbosity: Option<super::responses::TextVerbosity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apply_patch_tool_type: Option<ApplyPatchToolType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_search_tool_type: Option<WebSearchToolType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncation_policy: Option<TruncationPolicyConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_image_detail_original: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_compact_token_limit: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comp_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_context_window_percent: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experimental_supported_tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_search_tool: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_experimental_context: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_responses_lite: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_reasoning_effort_updates: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_repl_auto_review_required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_repl_disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_review_model_override: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_specialty: Option<String>,
    #[serde(
        default,
        deserialize_with = "optional_selector",
        skip_serializing_if = "Option::is_none"
    )]
    pub tool_mode: Option<ToolMode>,
    #[serde(
        default,
        deserialize_with = "optional_selector",
        skip_serializing_if = "Option::is_none"
    )]
    pub multi_agent_version: Option<MultiAgentVersion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi_agent_reasoning_effort: Option<ModelReasoningEffort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_instructions: Option<String>,
    // Fields also advertised by the bundled Codex catalog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefer_websockets: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_parallel_tool_calls: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_reasoning_summaries: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_sandboxed_review: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_in_plans: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimal_client_version: Option<ClientVersion>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
pub struct ModelReasoningLevel {
    pub effort: ModelReasoningEffort,
    pub description: String,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
pub struct ModelServiceTier {
    pub id: String,
    pub name: String,
    pub description: String,
}

fn reasoning_levels<'de, D>(deserializer: D) -> Result<Option<Vec<ModelReasoningLevel>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Level {
        Preset(ModelReasoningLevel),
        Effort(ModelReasoningEffort),
    }
    Ok(
        Option::<Vec<Level>>::deserialize(deserializer)?.map(|levels| {
            levels
                .into_iter()
                .map(|level| match level {
                    Level::Preset(preset) => preset,
                    Level::Effort(effort) => ModelReasoningLevel {
                        effort,
                        description: String::new(),
                    },
                })
                .collect()
        }),
    )
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct ModelMessages {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_filter_guidance: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistent_instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolMessages>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions_template: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions_variables: Option<ModelInstructionsVariables>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approvals: Option<ApprovalMessages>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collaboration_modes: Option<CollaborationModeMessages>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_review: Option<AutoReviewMessages>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionMessages>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi_agent: Option<MultiAgentMessages>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<ModelTokenBudgetConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guardian_v2: Option<GuardianV2ModelConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmation_policies: Option<ConfirmationPolicies>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct ConfirmationPolicies {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_use: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub computer_use: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct ToolMessages {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indirect_description_prefixes: Option<IndirectDescriptionPrefixes>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_user_message_async: Option<ToolMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi_agent: Option<MultiAgentToolMessages>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_mode: Option<CodeModeToolMessages>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_resources: Option<McpResourceToolMessages>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct IndirectDescriptionPrefixes {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespaces: Option<std::collections::BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<std::collections::BTreeMap<String, String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct ToolMessage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct MultiAgentToolMessages {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spawn_agent: Option<ToolMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_message: Option<ToolMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub followup_task: Option<ToolMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_agent: Option<ToolMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interrupt_agent: Option<ToolMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_agents: Option<ToolMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub create_channel: Option<ToolMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub get_channels: Option<ToolMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_threads: Option<ToolMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_posts: Option<ToolMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_thread: Option<ToolMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_post: Option<ToolMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscribe: Option<ToolMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unsubscribe: Option<ToolMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<ToolMessage>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct McpResourceToolMessages {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_mcp_resources: Option<ToolMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_mcp_resource_templates: Option<ToolMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_mcp_resource: Option<ToolMessage>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct CodeModeToolMessages {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exec: Option<ToolMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait: Option<ToolMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deferred_nested_tools_guidance: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_typescript_preamble: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct ModelTokenBudgetConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub use_history_notes_extension: bool,
    pub reminder_threshold_tokens: i64,
    pub reminder_message_template: String,
    pub guidance_message: String,
    pub auto_compact_fallback_prompt: String,
    pub auto_compact_fallback_buffer_tokens: i64,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct ApprovalMessages {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_request: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_request_auto_review: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub never: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unless_trusted: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct CollaborationModeMessages {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct AutoReviewMessages {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_template: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_repl_policy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejection_instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_instructions: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct PermissionMessages {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub danger_full_access: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_write: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct MultiAgentMessages {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<MultiAgentRoleMessages>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<MultiAgentModeMessages>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct MultiAgentRoleMessages {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct MultiAgentModeMessages {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explicit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proactive: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint_text: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct ModelInstructionsVariables {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub personality_default: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub personality_friendly: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub personality_pragmatic: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct ModelInfoUpgrade {
    pub model: String,
    pub migration_markdown: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retirement_at: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct ModelAvailabilityNux {
    pub message: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct GuardianModelPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub computer_use: Option<GuardianReviewMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<GuardianReviewMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_changes: Option<GuardianReviewMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<GuardianReviewMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<GuardianReviewMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<GuardianReviewMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other_tools: Option<GuardianReviewMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unscored_action: Option<GuardianUnscoredAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_cua_call: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandboxed_exec_commands: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct GuardianV2ModelConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub async_classifier_mode: Option<AsyncClassifierMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub async_classifier_conversation_token_limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classifier_instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_threshold_basis_points: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_call_lag: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ModelReasoningEffort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<GuardianV2TranscriptModelConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_action_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_classifier_instruction_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reuse_parent_compaction: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_parent_compaction_tokens: Option<usize>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct GuardianV2TranscriptModelConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_images: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_message_entry_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_entry_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_message_transcript_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_transcript_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_recent_non_user_entries: Option<usize>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct ModelAccessPrograms {
    #[serde(deserialize_with = "known_cyber_programs")]
    pub cyber: Vec<CyberAccessProgram>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
pub enum ModelVisibility {
    List,
    Hide,
    None,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
pub enum ConfigShellToolType {
    #[serde(alias = "default", alias = "local", alias = "shell_command")]
    UnifiedExec,
    Disabled,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
pub enum ApplyPatchToolType {
    Freeform,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
pub enum WebSearchToolType {
    Text,
    TextAndImage,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
pub enum TruncationMode {
    Bytes,
    Tokens,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
pub struct TruncationPolicyConfig {
    pub mode: TruncationMode,
    pub limit: i64,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
pub enum ToolMode {
    Direct,
    CodeMode,
    CodeModeOnly,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
pub enum MultiAgentVersion {
    Disabled,
    V1,
    V2,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
pub enum CyberAccessProgram {
    Standard,
    DaybreakBlue,
    DaybreakRed,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
pub enum GuardianReviewMode {
    Disabled,
    Synchronous,
    Adaptive,
    #[serde(untagged)]
    Unknown(String),
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
pub enum GuardianUnscoredAction {
    Ignore,
    AgeScore,
    InvalidateScore,
    #[serde(untagged)]
    Unknown(String),
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
pub enum AsyncClassifierMode {
    Snapshot,
    Conversation,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
pub enum ModelReasoningSummary {
    Auto,
    Concise,
    Detailed,
    None,
}

fn optional_selector<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    Ok(Option::<String>::deserialize(deserializer)?
        .and_then(|value| serde_json::from_value(serde_json::Value::String(value)).ok()))
}

fn known_cyber_programs<'de, D>(deserializer: D) -> Result<Vec<CyberAccessProgram>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Vec::<String>::deserialize(deserializer)?
        .into_iter()
        .filter_map(|value| serde_json::from_value(serde_json::Value::String(value)).ok())
        .collect())
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(untagged)]
pub enum ClientVersion {
    Text(String),
    Components([u32; 3]),
}

/// Catalog effort choices include CLI modes such as Ultra and Persistent;
/// they are distinct from the Responses request's reasoning-effort parameter.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
pub enum ModelReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    Ultra,
    Persistent,
    #[serde(untagged)]
    Custom(String),
}
