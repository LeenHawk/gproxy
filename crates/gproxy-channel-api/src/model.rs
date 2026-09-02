#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: Option<String>,
    pub context_window: Option<i64>,
    pub max_output_tokens: Option<i64>,
    pub thinking_supported: Option<bool>,
    pub thinking_adaptive_supported: Option<bool>,
    pub thinking_enabled_supported: Option<bool>,
}
