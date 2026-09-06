pub use gproxy_protocol::openai::{ModelReasoningLevel, ModelServiceTier};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: Option<String>,
    pub context_window: Option<i64>,
    pub max_output_tokens: Option<i64>,
    pub thinking_supported: Option<bool>,
    pub thinking_adaptive_supported: Option<bool>,
    pub thinking_enabled_supported: Option<bool>,
    #[serde(default)]
    pub metadata: ModelMetadata,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub description: Option<String>,
    pub instructions: Option<String>,
    pub max_context_window: Option<i64>,
    pub input_modalities: Option<Vec<String>>,
    pub output_modalities: Option<Vec<String>>,
    pub supported_parameters: Option<Vec<String>>,
    pub reasoning_levels: Option<Vec<ModelReasoningLevel>>,
    pub default_reasoning_level: Option<String>,
    pub service_tiers: Option<Vec<ModelServiceTier>>,
    pub default_service_tier: Option<String>,
    pub generation_methods: Option<Vec<String>>,
    pub supported_actions: Option<Vec<String>>,
    pub shell_type: Option<String>,
    pub support_verbosity: Option<bool>,
    pub default_verbosity: Option<String>,
    pub supports_reasoning_summary_parameter: Option<bool>,
    pub default_reasoning_summary: Option<String>,
    pub apply_patch_tool_type: Option<String>,
    pub web_search_tool_type: Option<String>,
    pub truncation_mode: Option<String>,
    pub truncation_limit: Option<i64>,
    pub auto_compact_token_limit: Option<i64>,
    pub effective_context_window_percent: Option<i64>,
    pub batch_supported: Option<bool>,
    pub citations_supported: Option<bool>,
    pub code_execution_supported: Option<bool>,
    pub context_management_supported: Option<bool>,
    pub structured_outputs_supported: Option<bool>,
    pub pdf_input_supported: Option<bool>,
    pub supports_image_detail_original: Option<bool>,
    pub supports_search_tool: Option<bool>,
}
