use super::LiveContent;
use super::LiveInt64;
use crate::Rest;
use crate::gemini::FunctionCall;
use crate::gemini::count_tokens::ModalityTokenCount;
use crate::gemini::generation::ServiceTier;
use serde::{Deserialize, Serialize};

// The Developer API grounding and URL-context definitions are the same as
// GenerateContent. SDK retrieval_queries/source_flagging_uris are Vertex-only.
pub type LiveGroundingMetadata = crate::gemini::generate_content::GroundingMetadata;
pub type LiveUrlContextMetadata = crate::gemini::generate_content::UrlContextMetadata;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct BidiGenerateContentServerContent {
    #[serde(alias = "model_turn")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_turn: Option<LiveContent>,
    #[serde(alias = "generation_complete")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_complete: Option<bool>,
    #[serde(alias = "turn_complete")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_complete: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interrupted: Option<bool>,
    #[serde(alias = "grounding_metadata")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grounding_metadata: Option<LiveGroundingMetadata>,
    #[serde(alias = "input_transcription")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_transcription: Option<BidiGenerateContentTranscription>,
    #[serde(alias = "output_transcription")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_transcription: Option<BidiGenerateContentTranscription>,
    #[serde(alias = "interim_input_transcription")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interim_input_transcription: Option<BidiGenerateContentTranscription>,
    #[serde(alias = "url_context_metadata")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_context_metadata: Option<LiveUrlContextMetadata>,
    #[serde(alias = "waiting_for_input")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_for_input: Option<bool>,
    #[serde(alias = "turn_complete_reason")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_complete_reason: Option<TurnCompleteReason>,
    #[serde(alias = "interaction_status")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_status: Option<InteractionStatus>,
    #[serde(alias = "speech_state")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speech_state: Option<SpeechState>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct BidiGenerateContentToolCall {
    #[serde(alias = "function_calls")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_calls: Option<Vec<FunctionCall>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct BidiGenerateContentToolCallCancellation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct GoAway {
    #[serde(alias = "time_left")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_left: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct SessionResumptionUpdate {
    #[serde(alias = "new_handle")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_handle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resumable: Option<bool>,
    #[serde(alias = "last_consumed_client_message_index")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_consumed_client_message_index: Option<LiveInt64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct BidiGenerateContentTranscription {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished: Option<bool>,
    #[serde(alias = "language_code")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language_code: Option<String>,
    #[serde(alias = "speaker_label")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words: Option<Vec<WordInfo>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct WordInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub word: Option<String>,
    #[serde(alias = "start_offset")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<String>,
    #[serde(alias = "end_offset")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_offset: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct LiveUsageMetadata {
    #[serde(alias = "prompt_token_count")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_token_count: Option<i64>,
    #[serde(alias = "cached_content_token_count")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_content_token_count: Option<i64>,
    #[serde(alias = "response_token_count")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_token_count: Option<i64>,
    #[serde(alias = "tool_use_prompt_token_count")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_use_prompt_token_count: Option<i64>,
    #[serde(alias = "thoughts_token_count")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thoughts_token_count: Option<i64>,
    #[serde(alias = "total_token_count")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_token_count: Option<i64>,
    #[serde(alias = "prompt_tokens_details")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_details: Option<Vec<ModalityTokenCount>>,
    #[serde(alias = "cache_tokens_details")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_tokens_details: Option<Vec<ModalityTokenCount>>,
    #[serde(alias = "response_tokens_details")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_tokens_details: Option<Vec<ModalityTokenCount>>,
    #[serde(alias = "tool_use_prompt_tokens_details")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_use_prompt_tokens_details: Option<Vec<ModalityTokenCount>>,
    #[serde(alias = "service_tier")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct VoiceActivityDetectionSignal {
    #[serde(alias = "vad_signal_type")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vad_signal_type: Option<VadSignalType>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct VoiceActivity {
    #[serde(rename = "type")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_: Option<VoiceActivityType>,
    #[serde(alias = "audio_offset")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_offset: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum TurnCompleteReason {
    #[serde(rename = "TURN_COMPLETE_REASON_UNSPECIFIED")]
    TurnCompleteReasonUnspecified,
    #[serde(rename = "MALFORMED_FUNCTION_CALL")]
    MalformedFunctionCall,
    #[serde(rename = "RESPONSE_REJECTED")]
    ResponseRejected,
    #[serde(rename = "NEED_MORE_INPUT")]
    NeedMoreInput,
    #[serde(rename = "PROHIBITED_INPUT_CONTENT")]
    ProhibitedInputContent,
    #[serde(rename = "IMAGE_PROHIBITED_INPUT_CONTENT")]
    ImageProhibitedInputContent,
    #[serde(rename = "INPUT_TEXT_CONTAIN_PROMINENT_PERSON_PROHIBITED")]
    InputTextContainProminentPersonProhibited,
    #[serde(rename = "INPUT_IMAGE_CELEBRITY")]
    InputImageCelebrity,
    #[serde(rename = "INPUT_IMAGE_PHOTO_REALISTIC_CHILD_PROHIBITED")]
    InputImagePhotoRealisticChildProhibited,
    #[serde(rename = "INPUT_TEXT_NCII_PROHIBITED")]
    InputTextNciiProhibited,
    #[serde(rename = "INPUT_OTHER")]
    InputOther,
    #[serde(rename = "INPUT_IP_PROHIBITED")]
    InputIpProhibited,
    #[serde(rename = "BLOCKLIST")]
    Blocklist,
    #[serde(rename = "UNSAFE_PROMPT_FOR_IMAGE_GENERATION")]
    UnsafePromptForImageGeneration,
    #[serde(rename = "GENERATED_IMAGE_SAFETY")]
    GeneratedImageSafety,
    #[serde(rename = "GENERATED_CONTENT_SAFETY")]
    GeneratedContentSafety,
    #[serde(rename = "GENERATED_AUDIO_SAFETY")]
    GeneratedAudioSafety,
    #[serde(rename = "GENERATED_VIDEO_SAFETY")]
    GeneratedVideoSafety,
    #[serde(rename = "GENERATED_CONTENT_PROHIBITED")]
    GeneratedContentProhibited,
    #[serde(rename = "GENERATED_CONTENT_BLOCKLIST")]
    GeneratedContentBlocklist,
    #[serde(rename = "GENERATED_IMAGE_PROHIBITED")]
    GeneratedImageProhibited,
    #[serde(rename = "GENERATED_IMAGE_CELEBRITY")]
    GeneratedImageCelebrity,
    #[serde(rename = "GENERATED_IMAGE_PROMINENT_PEOPLE_DETECTED_BY_REWRITER")]
    GeneratedImageProminentPeopleDetectedByRewriter,
    #[serde(rename = "GENERATED_IMAGE_IDENTIFIABLE_PEOPLE")]
    GeneratedImageIdentifiablePeople,
    #[serde(rename = "GENERATED_IMAGE_MINORS")]
    GeneratedImageMinors,
    #[serde(rename = "OUTPUT_IMAGE_IP_PROHIBITED")]
    OutputImageIpProhibited,
    #[serde(rename = "GENERATED_OTHER")]
    GeneratedOther,
    #[serde(rename = "MAX_REGENERATION_REACHED")]
    MaxRegenerationReached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum InteractionStatus {
    #[serde(rename = "INTERACTION_STATUS_UNSPECIFIED")]
    InteractionStatusUnspecified,
    #[serde(rename = "IN_PROGRESS")]
    InProgress,
    #[serde(rename = "REQUIRES_ACTION")]
    RequiresAction,
    #[serde(rename = "IDLE")]
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum VadSignalType {
    #[serde(rename = "VAD_SIGNAL_TYPE_UNSPECIFIED")]
    VadSignalTypeUnspecified,
    #[serde(rename = "VAD_SIGNAL_TYPE_SOS")]
    VadSignalTypeSos,
    #[serde(rename = "VAD_SIGNAL_TYPE_EOS")]
    VadSignalTypeEos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum VoiceActivityType {
    #[serde(rename = "TYPE_UNSPECIFIED")]
    TypeUnspecified,
    #[serde(rename = "ACTIVITY_START")]
    ActivityStart,
    #[serde(rename = "ACTIVITY_END")]
    ActivityEnd,
}

/// ai.google.dev/api/live exposes deprecated speechState but provides no value
/// table; current public proto/SDK omit its declaration. Preserve the named enum
/// representation without inventing constants. ProtoJSON enums can be names/ints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum SpeechState {
    Name(String),
    Number(i32),
}
