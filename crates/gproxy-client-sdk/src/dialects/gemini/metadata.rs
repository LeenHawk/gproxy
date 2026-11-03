use super::*;
// Only native fields without a portable payload are persisted.
#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct MediaMetadata {
    pub(super) mime_type: Option<String>,
    pub(super) display_name: Option<String>,
    #[serde(flatten)]
    pub(super) rest: Rest,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct FunctionMetadata {
    pub(super) id_present: bool,
    pub(super) args_present: bool,
    #[serde(flatten)]
    pub(super) rest: Rest,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct ResultMetadata {
    pub(super) id_present: bool,
    pub(super) parts_present: bool,
    pub(super) will_continue: Option<bool>,
    pub(super) scheduling: Option<g::Scheduling>,
    #[serde(flatten)]
    pub(super) rest: Rest,
}

#[derive(Serialize, Deserialize)]
pub(super) struct ServerCallMetadata {
    pub(super) tool_type: g::ToolType,
    pub(super) id_present: bool,
    pub(super) name_present: bool,
    pub(super) args_present: bool,
    #[serde(default, flatten)]
    pub(super) rest: Rest,
}

#[derive(Serialize, Deserialize)]
pub(super) struct ServerResponseMetadata {
    pub(super) tool_type: g::ToolType,
    pub(super) id_present: bool,
    pub(super) response_present: bool,
    #[serde(default, flatten)]
    pub(super) rest: Rest,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct AudioMetadata {
    pub(super) speaker_label: Option<String>,
    pub(super) words: Option<Vec<g::audio::WordInfo>>,
    #[serde(flatten)]
    pub(super) rest: Rest,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct PartFields {
    pub(super) thought: Option<bool>,
    pub(super) thought_signature: Option<String>,
    pub(super) part_metadata: Option<Rest>,
    pub(super) media_resolution: Option<g::MediaResolution>,
    pub(super) media_processing: Option<g::MediaProcessing>,
    pub(super) speech_metadata: Option<g::audio::SpeechMetadata>,
    pub(super) video_metadata: Option<g::VideoMetadata>,
    #[serde(flatten)]
    pub(super) rest: Rest,
}

pub(super) fn part_fields(part: g::Part) -> PartFields {
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

pub(super) fn native_part(meta: PartFields) -> g::Part {
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

pub(super) fn audio_metadata(value: g::audio::AudioTranscription) -> AudioMetadata {
    AudioMetadata {
        speaker_label: value.speaker_label,
        words: value.words,
        rest: value.rest,
    }
}

pub(super) fn transcription(text: String, meta: AudioMetadata) -> g::audio::AudioTranscription {
    let mut value = g::audio::AudioTranscription::builder(text).build();
    value.speaker_label = meta.speaker_label;
    value.words = meta.words;
    value.rest = meta.rest;
    value
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct PartMetadata {
    pub(super) inline_data: Option<MediaMetadata>,
    pub(super) file_data: Option<MediaMetadata>,
    pub(super) function_call: Option<FunctionMetadata>,
    pub(super) function_response: Option<ResultMetadata>,
    pub(super) audio_transcription: Option<AudioMetadata>,
    pub(super) response_json_text: bool,
    pub(super) tool_call: Option<ServerCallMetadata>,
    pub(super) tool_response: Option<ServerResponseMetadata>,
    pub(super) trailing_transcripts: Vec<TranscriptMetadata>,
    pub(super) primary_transcript_chars: Option<usize>,
    #[serde(flatten)]
    pub(super) fields: PartFields,
}

#[derive(Serialize, Deserialize)]
pub(super) struct TranscriptMetadata {
    pub(super) fields: PartFields,
    pub(super) transcription: AudioMetadata,
    pub(super) chars: usize,
}

#[derive(Default, Serialize, Deserialize)]
pub(super) struct ResponseFields {
    #[serde(default, flatten)]
    pub(super) rest: Rest,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ResponsePartMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) inline_data: Option<MediaMetadata>,
    #[serde(flatten)]
    pub(super) fields: ResponseFields,
}
