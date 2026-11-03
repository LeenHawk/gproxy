use super::*;
#[derive(Serialize, Deserialize)]
pub(super) struct TextMetadata {
    #[serde(default)]
    pub(super) logprobs: Vec<i::OutputLogprob>,
    #[serde(default, flatten)]
    pub(super) rest: Rest,
}

#[derive(Serialize, Deserialize)]
pub(super) struct PartMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) native: Option<TextMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) item: Option<MessageMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) group: Option<u32>,
}

pub(super) type Agent = gproxy_protocol::openai::responses::multi_agent::Agent;

pub(super) fn nullable<'de, D, T>(
    deserializer: D,
) -> std::result::Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct MessageMetadata {
    pub(super) id: String,
    pub(super) status: i::OutputMessageStatus,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) agent: Option<Option<Agent>>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) phase: Option<Option<i::MessagePhase>>,
    #[serde(default, flatten)]
    pub(super) rest: Rest,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Remainder {
    #[serde(flatten)]
    pub(super) rest: Rest,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct InputImageMetadata {
    pub(super) file_id_null: bool,
    pub(super) image_url_null: bool,
    #[serde(flatten)]
    pub(super) rest: Rest,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct FileMetadata {
    pub(super) raw_base64: bool,
    pub(super) file_id_null: bool,
    pub(super) detail_present: bool,
    #[serde(flatten)]
    pub(super) rest: Rest,
}

#[derive(Serialize, Deserialize)]
pub(super) struct GeneratedImageMetadata {
    pub(super) id: String,
    pub(super) status: i::ImageGenerationStatus,
    pub(super) error: Option<gproxy_protocol::openai::responses::errors::ImageGenerationCallError>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) agent: Option<Option<Agent>>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) action: Option<Option<t::ImageAction>>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) background: Option<Option<t::ImageBackground>>,
    pub(super) format_present: bool,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) quality: Option<Option<t::ImageQuality>>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) revised_prompt: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) size: Option<Option<String>>,
    #[serde(default, flatten)]
    pub(super) rest: Rest,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "fields", rename_all = "snake_case")]
// Read only at metadata serialization boundaries; boxing adds no lasting saving.
#[allow(clippy::large_enum_variant)]
pub(super) enum ImageMetadata {
    Input(InputImageMetadata),
    Generated(GeneratedImageMetadata),
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct FunctionMetadata {
    pub(super) id: Option<String>,
    pub(super) namespace: Option<String>,
    pub(super) status: Option<i::ItemStatus>,
    pub(super) async_: Option<bool>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) agent: Option<Option<Agent>>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) caller: Option<Option<i::Caller>>,
    #[serde(flatten)]
    pub(super) rest: Rest,
}

#[derive(Serialize, Deserialize)]
pub(super) struct ResultMetadata {
    pub(super) id: String,
    pub(super) status: i::ItemStatus,
    pub(super) name_present: bool,
    pub(super) call_id_present: bool,
    pub(super) namespace: Option<String>,
    pub(super) created_by: Option<String>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) agent: Option<Option<Agent>>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) caller: Option<Option<i::Caller>>,
    #[serde(default, flatten)]
    pub(super) rest: Rest,
}

#[derive(Serialize, Deserialize)]
pub(super) struct SummaryMetadata {
    pub(super) chars: usize,
    #[serde(default, flatten)]
    pub(super) rest: Rest,
}

#[derive(Serialize, Deserialize)]
pub(super) struct ReasoningMetadata {
    pub(super) id: String,
    pub(super) summary: Vec<SummaryMetadata>,
    pub(super) content: Option<Vec<i::ReasoningContent>>,
    pub(super) status: Option<i::ReasoningStatus>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) agent: Option<Option<Agent>>,
    #[serde(
        default,
        deserialize_with = "nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) encrypted_content: Option<Option<String>>,
    #[serde(default, flatten)]
    pub(super) rest: Rest,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum ServerMetadata {
    Web {
        status: i::WebSearchStatus,
        #[serde(
            default,
            deserialize_with = "nullable",
            skip_serializing_if = "Option::is_none"
        )]
        agent: Option<Option<Agent>>,
        rest: Rest,
    },
    Files {
        status: i::FileSearchStatus,
        #[serde(
            default,
            deserialize_with = "nullable",
            skip_serializing_if = "Option::is_none"
        )]
        agent: Option<Option<Agent>>,
        rest: Rest,
    },
    Code {
        status: i::CodeInterpreterStatus,
        container_id: String,
        #[serde(
            default,
            deserialize_with = "nullable",
            skip_serializing_if = "Option::is_none"
        )]
        agent: Option<Option<Agent>>,
        rest: Rest,
    },
    Computer {
        id: String,
        status: i::ItemStatus,
        pending_safety_checks: Vec<i::PendingSafetyCheck>,
        #[serde(
            default,
            deserialize_with = "nullable",
            skip_serializing_if = "Option::is_none"
        )]
        agent: Option<Option<Agent>>,
        rest: Rest,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ServerOutputKind {
    FileSearch,
    CodeInterpreter,
}

#[derive(Serialize, Deserialize)]
pub(super) struct ServerOutputMetadata {
    pub(super) kind: ServerOutputKind,
}

#[derive(Serialize, Deserialize)]
pub(super) struct ComputerArguments {
    pub(super) action: Option<i::ComputerAction>,
    pub(super) actions: Option<Vec<i::ComputerAction>>,
}
