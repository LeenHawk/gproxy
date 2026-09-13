use super::content::*;
use crate::Rest;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ThinkingLevel {
    #[serde(rename = "THINKING_LEVEL_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "MINIMAL")]
    Minimal,
    #[serde(rename = "LOW")]
    Low,
    #[serde(rename = "MEDIUM")]
    Medium,
    #[serde(rename = "HIGH")]
    High,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum GenerationMediaResolution {
    #[serde(rename = "MEDIA_RESOLUTION_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "MEDIA_RESOLUTION_LOW")]
    Low,
    #[serde(rename = "MEDIA_RESOLUTION_MEDIUM")]
    Medium,
    #[serde(rename = "MEDIA_RESOLUTION_HIGH")]
    High,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum TextMimeType {
    #[serde(rename = "MIME_TYPE_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "APPLICATION_JSON")]
    ApplicationJson,
    #[serde(rename = "TEXT_PLAIN")]
    TextPlain,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum AudioMimeType {
    #[serde(rename = "MIME_TYPE_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "AUDIO_MP3")]
    AudioMp3,
    #[serde(rename = "AUDIO_OGG_OPUS")]
    AudioOggOpus,
    #[serde(rename = "AUDIO_L16")]
    AudioL16,
    #[serde(rename = "AUDIO_WAV")]
    AudioWav,
    #[serde(rename = "AUDIO_ALAW")]
    AudioAlaw,
    #[serde(rename = "AUDIO_MULAW")]
    AudioMulaw,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum Delivery {
    #[serde(rename = "DELIVERY_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "INLINE")]
    Inline,
    #[serde(rename = "URI")]
    Uri,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ImageMimeType {
    #[serde(rename = "MIME_TYPE_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "IMAGE_JPEG")]
    ImageJpeg,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum AspectRatio {
    #[serde(rename = "ASPECT_RATIO_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "ASPECT_RATIO_ONE_BY_ONE")]
    OneByOne,
    #[serde(rename = "ASPECT_RATIO_TWO_BY_THREE")]
    TwoByThree,
    #[serde(rename = "ASPECT_RATIO_THREE_BY_TWO")]
    ThreeByTwo,
    #[serde(rename = "ASPECT_RATIO_THREE_BY_FOUR")]
    ThreeByFour,
    #[serde(rename = "ASPECT_RATIO_FOUR_BY_THREE")]
    FourByThree,
    #[serde(rename = "ASPECT_RATIO_FOUR_BY_FIVE")]
    FourByFive,
    #[serde(rename = "ASPECT_RATIO_FIVE_BY_FOUR")]
    FiveByFour,
    #[serde(rename = "ASPECT_RATIO_NINE_BY_SIXTEEN")]
    NineBySixteen,
    #[serde(rename = "ASPECT_RATIO_SIXTEEN_BY_NINE")]
    SixteenByNine,
    #[serde(rename = "ASPECT_RATIO_TWENTY_ONE_BY_NINE")]
    TwentyOneByNine,
    #[serde(rename = "ASPECT_RATIO_ONE_BY_EIGHT")]
    OneByEight,
    #[serde(rename = "ASPECT_RATIO_EIGHT_BY_ONE")]
    EightByOne,
    #[serde(rename = "ASPECT_RATIO_ONE_BY_FOUR")]
    OneByFour,
    #[serde(rename = "ASPECT_RATIO_FOUR_BY_ONE")]
    FourByOne,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ImageSize {
    #[serde(rename = "IMAGE_SIZE_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "IMAGE_SIZE_FIVE_TWELVE")]
    FiveTwelve,
    #[serde(rename = "IMAGE_SIZE_ONE_K")]
    OneK,
    #[serde(rename = "IMAGE_SIZE_TWO_K")]
    TwoK,
    #[serde(rename = "IMAGE_SIZE_FOUR_K")]
    FourK,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ServiceTier {
    #[serde(rename = "unspecified")]
    Unspecified,
    #[serde(rename = "standard")]
    Standard,
    #[serde(rename = "flex")]
    Flex,
    #[serde(rename = "priority")]
    Priority,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct SpeechConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "voice_config")]
    pub voice_config: Option<VoiceConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "multi_speaker_voice_config")]
    pub multi_speaker_voice_config: Option<MultiSpeakerVoiceConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "language_code")]
    pub language_code: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct VoiceConfig {
    #[serde(alias = "prebuilt_voice_config")]
    pub prebuilt_voice_config: PrebuiltVoiceConfig,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct PrebuiltVoiceConfig {
    #[serde(alias = "voice_name")]
    pub voice_name: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct MultiSpeakerVoiceConfig {
    #[serde(alias = "speaker_voice_configs")]
    pub speaker_voice_configs: Vec<SpeakerVoiceConfig>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct SpeakerVoiceConfig {
    pub speaker: String,
    #[serde(alias = "voice_config")]
    pub voice_config: VoiceConfig,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ThinkingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "thinking_budget")]
    pub thinking_budget: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "thinking_level")]
    pub thinking_level: Option<ThinkingLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "include_thoughts")]
    pub include_thoughts: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ImageConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "aspect_ratio")]
    pub aspect_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "image_size")]
    pub image_size: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseFormatConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<TextResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<ImageResponseFormat>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct TextResponseFormat {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "mime_type")]
    pub mime_type: Option<TextMimeType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct AudioResponseFormat {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "mime_type")]
    pub mime_type: Option<AudioMimeType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery: Option<Delivery>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "sample_rate")]
    pub sample_rate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "bit_rate")]
    pub bit_rate: Option<i64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ImageResponseFormat {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "mime_type")]
    pub mime_type: Option<ImageMimeType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery: Option<Delivery>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "aspect_ratio")]
    pub aspect_ratio: Option<AspectRatio>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "image_size")]
    pub image_size: Option<ImageSize>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
/// Full `GenerateContentRequest` object accepted inside CountTokens. Its
/// `model` is part of this embedded Batch API object; standalone REST
/// generation keeps model in its path.
pub struct EmbeddedGenerateContentRequest {
    /// Required by the Batch API `GenerateContentRequest` embedded in CountTokens.
    pub model: String,
    pub contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "tool_config")]
    pub tool_config: Option<ToolConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "safety_settings")]
    pub safety_settings: Option<Vec<SafetySetting>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "system_instruction")]
    pub system_instruction: Option<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "generation_config")]
    pub generation_config: Option<GenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "cached_content")]
    pub cached_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "service_tier")]
    pub service_tier: Option<ServiceTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "stop_sequences")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "response_mime_type")]
    pub response_mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "response_schema")]
    pub response_schema: Option<Schema>,
    #[serde(
        rename = "_responseJsonSchema",
        skip_serializing_if = "Option::is_none"
    )]
    pub response_json_schema_internal: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "response_json_schema")]
    pub response_json_schema: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "response_modalities")]
    pub response_modalities: Option<Vec<Modality>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "candidate_count")]
    pub candidate_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "max_output_tokens")]
    pub max_output_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "top_p")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "top_k")]
    pub top_k: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "presence_penalty")]
    pub presence_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "frequency_penalty")]
    pub frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "response_logprobs")]
    pub response_logprobs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "enable_enhanced_civic_answers")]
    pub enable_enhanced_civic_answers: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "speech_config")]
    pub speech_config: Option<SpeechConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "thinking_config")]
    pub thinking_config: Option<ThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "image_config")]
    pub image_config: Option<ImageConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "media_resolution")]
    pub media_resolution: Option<GenerationMediaResolution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "response_format")]
    pub response_format: Option<ResponseFormatConfig>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
