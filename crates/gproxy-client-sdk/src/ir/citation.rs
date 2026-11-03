use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TextRange {
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum DocumentRange {
    Pages {
        start: u32,
        end: u32,
    },
    Chars {
        start: u64,
        end: u64,
    },
    Chunks {
        start: u32,
        end: u32,
    },
    #[serde(untagged)]
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum DocumentReference {
    Block {
        document: Box<DocumentBlock>,
    },
    Index {
        vendor: Vendor,
        index: u32,
        file_id: Option<String>,
    },
    ProviderFile {
        vendor: Vendor,
        id: String,
    },
    #[serde(untagged)]
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum CitationSource {
    Url {
        url: String,
    },
    Document {
        document: DocumentReference,
        range: Option<DocumentRange>,
    },
    #[serde(untagged)]
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Citation {
    pub source: CitationSource,
    pub title: Option<String>,
    pub cited_text: Option<String>,
    pub text_range: Option<TextRange>,
    #[serde(default)]
    pub provider_metadata: ProviderMetadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Source {
    /// None when a retrieved snippet has no URL or file identity.
    pub source: Option<CitationSource>,
    pub title: Option<String>,
    pub text: Option<String>,
    #[serde(default)]
    pub provider_metadata: ProviderMetadata,
}
