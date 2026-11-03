use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum CacheTtl {
    FiveMinutes,
    ThirtyMinutes,
    OneHour,
    TwelveHours,
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CacheBreakpoint {
    pub ttl: Option<CacheTtl>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Modality {
    Text,
    Audio,
    Image,
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SpeechSettings {
    pub voice: String,
    pub format: AudioFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ReasoningEffort {
    ProviderDefault,
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ReasoningSettings {
    pub effort: ReasoningEffort,
    pub token_budget: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Settings {
    #[serde(default)]
    pub reasoning: Option<ReasoningSettings>,
    #[serde(default)]
    pub output_modalities: Vec<Modality>,
    pub speech: Option<SpeechSettings>,
    pub max_output_tokens: Option<u32>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    #[serde(default)]
    pub stop_sequences: Vec<String>,
    pub tool_choice: Option<ToolChoice>,
    pub output_schema: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GenerateRequest {
    pub model: Model,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub tools: Vec<Tool>,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub provider_options: ProviderOptions,
}

impl GenerateRequest {
    pub fn new(model: Model, messages: Vec<Message>) -> Self {
        Self {
            model,
            messages,
            tools: Vec::new(),
            settings: Settings::default(),
            provider_options: ProviderOptions::default(),
        }
    }
}
