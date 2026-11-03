use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Asset {
    Inline {
        bytes: Vec<u8>,
    },
    Url {
        url: String,
    },
    Reference {
        reference: String,
    },
    ProviderFile {
        vendor: Vendor,
        id: String,
    },
    #[serde(untagged)]
    Unknown(Value),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Detail {
    Low,
    Medium,
    High,
    #[default]
    Auto,
    Original,
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Image {
    pub source: Asset,
    pub media_type: Option<String>,
    #[serde(default)]
    pub detail: Detail,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum DocumentSource {
    Asset {
        source: Asset,
    },
    Text {
        text: String,
    },
    Chunks {
        chunks: Vec<DocumentChunk>,
    },
    #[serde(untagged)]
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Document {
    pub source: DocumentSource,
    pub media_type: Option<String>,
    pub title: Option<String>,
    pub context: Option<String>,
    pub citations: Option<bool>,
    #[serde(default)]
    pub detail: Detail,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct File {
    pub source: Asset,
    pub media_type: Option<String>,
    pub filename: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum AudioFormat {
    Encoded {
        media_type: String,
    },
    Pcm {
        media_type: String,
        sample_rate: u32,
        channels: u16,
    },
    #[serde(untagged)]
    Unknown(Value),
}

impl AudioFormat {
    pub fn media_type(&self) -> Option<&str> {
        match self {
            Self::Encoded { media_type } | Self::Pcm { media_type, .. } => Some(media_type),
            Self::Unknown(_) => None,
        }
    }
    pub fn pcm(sample_rate: u32, channels: u16) -> Self {
        Self::Pcm {
            media_type: "audio/pcm".into(),
            sample_rate,
            channels,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Audio {
    pub source: Asset,
    pub format: AudioFormat,
    pub transcript: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct VideoClip {
    pub start_seconds: Option<f64>,
    pub end_seconds: Option<f64>,
    pub fps: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Video {
    pub source: Asset,
    pub media_type: Option<String>,
    pub clip: Option<VideoClip>,
}
