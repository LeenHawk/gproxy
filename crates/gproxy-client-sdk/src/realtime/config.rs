use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum RealtimeDialect {
    Openai,
    Gemini,
    #[serde(untagged)]
    Unknown(String),
}
impl RealtimeDialect {
    pub(crate) fn vendor(&self) -> Result<Vendor> {
        match self {
            Self::Openai => Ok(Vendor::Openai),
            Self::Gemini => Ok(Vendor::Gemini),
            Self::Unknown(_) => Err(Error::unsupported("Realtime supports OpenAI and Gemini")),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RealtimeModel {
    pub id: String,
    pub dialect: RealtimeDialect,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum TurnDetection {
    ServerVad,
    SemanticVad,
    Manual,
    #[serde(untagged)]
    Unknown(String),
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RealtimeInputTranscription {
    pub model: Option<String>,
    pub language: Option<String>,
    pub prompt: Option<String>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RealtimeOptions {
    pub openai: openai::OpenaiRealtimeOptions,
    pub gemini: gemini::GeminiRealtimeOptions,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RealtimeConfig {
    pub model: RealtimeModel,
    pub instructions: Option<String>,
    pub voice: Option<String>,
    pub input_audio_format: AudioFormat,
    pub output_audio_format: AudioFormat,
    pub output_modalities: Vec<Modality>,
    pub turn_detection: TurnDetection,
    pub tools: Vec<Tool>,
    pub tool_choice: Option<ToolChoice>,
    pub input_transcription: Option<RealtimeInputTranscription>,
    pub history: Option<Conversation>,
    /// Automatically request a response after portable text/tool-result sends.
    pub auto_response: bool,
    #[serde(default)]
    pub provider_options: RealtimeOptions,
}
impl RealtimeConfig {
    pub fn new(model: RealtimeModel) -> Self {
        let input_rate = if model.dialect == RealtimeDialect::Gemini {
            16000
        } else {
            24000
        };
        Self {
            model,
            instructions: None,
            voice: None,
            input_audio_format: AudioFormat::pcm(input_rate, 1),
            output_audio_format: AudioFormat::pcm(24000, 1),
            output_modalities: vec![Modality::Audio],
            turn_detection: TurnDetection::ServerVad,
            tools: Vec::new(),
            tool_choice: None,
            input_transcription: None,
            history: None,
            auto_response: true,
            provider_options: RealtimeOptions::default(),
        }
    }
}
