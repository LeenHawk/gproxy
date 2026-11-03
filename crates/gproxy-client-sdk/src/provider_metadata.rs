use crate::protocol::{Rest, gemini as g};
use serde::{Deserialize, Serialize};
// Only native fields without a portable payload are persisted.
#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct MediaMetadata {
    pub(crate) mime_type: Option<String>,
    pub(crate) display_name: Option<String>,
    #[serde(flatten)]
    pub(crate) rest: Rest,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FunctionMetadata {
    pub(crate) id_present: bool,
    pub(crate) args_present: bool,
    #[serde(flatten)]
    pub(crate) rest: Rest,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ResultMetadata {
    pub(crate) id_present: bool,
    pub(crate) parts_present: bool,
    pub(crate) will_continue: Option<bool>,
    pub(crate) scheduling: Option<g::Scheduling>,
    #[serde(flatten)]
    pub(crate) rest: Rest,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct ServerCallMetadata {
    pub(crate) tool_type: g::ToolType,
    pub(crate) id_present: bool,
    pub(crate) name_present: bool,
    pub(crate) args_present: bool,
    #[serde(default, flatten)]
    pub(crate) rest: Rest,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct ServerResponseMetadata {
    pub(crate) tool_type: g::ToolType,
    pub(crate) id_present: bool,
    pub(crate) response_present: bool,
    #[serde(default, flatten)]
    pub(crate) rest: Rest,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct AudioMetadata {
    pub(crate) speaker_label: Option<String>,
    pub(crate) words: Option<Vec<g::audio::WordInfo>>,
    #[serde(flatten)]
    pub(crate) rest: Rest,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct PartFields {
    pub(crate) thought: Option<bool>,
    pub(crate) thought_signature: Option<String>,
    pub(crate) part_metadata: Option<Rest>,
    pub(crate) media_resolution: Option<g::MediaResolution>,
    pub(crate) media_processing: Option<g::MediaProcessing>,
    pub(crate) speech_metadata: Option<g::audio::SpeechMetadata>,
    pub(crate) video_metadata: Option<g::VideoMetadata>,
    #[serde(flatten)]
    pub(crate) rest: Rest,
}

pub(crate) fn part_fields(part: g::Part) -> PartFields {
    PartFields {
        thought: part.thought,
        thought_signature: part.thought_signature,
        part_metadata: part.part_metadata,
        media_resolution: part.media_resolution,
        media_processing: part.media_processing,
        speech_metadata: part.speech_metadata,
        video_metadata: part.video_metadata,
        rest: part.rest,
    }
}

#[cfg(any(feature = "gemini", feature = "images"))]
pub(crate) fn native_part(meta: PartFields) -> g::Part {
    let mut part = g::Part::builder().build();
    part.thought = meta.thought;
    part.thought_signature = meta.thought_signature;
    part.part_metadata = meta.part_metadata;
    part.media_resolution = meta.media_resolution;
    part.media_processing = meta.media_processing;
    part.speech_metadata = meta.speech_metadata;
    part.video_metadata = meta.video_metadata;
    part.rest = meta.rest;
    part
}

#[cfg(any(feature = "gemini", feature = "speech", feature = "transcription"))]
pub(crate) fn audio_metadata(value: g::audio::AudioTranscription) -> AudioMetadata {
    AudioMetadata {
        speaker_label: value.speaker_label,
        words: value.words,
        rest: value.rest,
    }
}

#[cfg(feature = "gemini")]
pub(crate) fn transcription(text: String, meta: AudioMetadata) -> g::audio::AudioTranscription {
    let mut value = g::audio::AudioTranscription::builder(text).build();
    value.speaker_label = meta.speaker_label;
    value.words = meta.words;
    value.rest = meta.rest;
    value
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct PartMetadata {
    pub(crate) inline_data: Option<MediaMetadata>,
    pub(crate) file_data: Option<MediaMetadata>,
    pub(crate) function_call: Option<FunctionMetadata>,
    pub(crate) function_response: Option<ResultMetadata>,
    pub(crate) audio_transcription: Option<AudioMetadata>,
    pub(crate) response_json_text: bool,
    pub(crate) tool_call: Option<ServerCallMetadata>,
    pub(crate) tool_response: Option<ServerResponseMetadata>,
    pub(crate) trailing_transcripts: Vec<TranscriptMetadata>,
    pub(crate) primary_transcript_chars: Option<usize>,
    #[serde(flatten)]
    pub(crate) fields: PartFields,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct TranscriptMetadata {
    pub(crate) fields: PartFields,
    pub(crate) transcription: AudioMetadata,
    pub(crate) chars: usize,
}

#[derive(Default, Serialize, Deserialize)]
#[cfg(feature = "gemini")]
pub(crate) struct ResponseFields {
    #[serde(default, flatten)]
    pub(crate) rest: Rest,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg(feature = "gemini")]
pub(crate) struct ResponsePartMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) inline_data: Option<MediaMetadata>,
    #[serde(flatten)]
    pub(crate) fields: ResponseFields,
}
