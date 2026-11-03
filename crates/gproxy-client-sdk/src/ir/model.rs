use super::*;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Vendor {
    Claude,
    Openai,
    Gemini,
    #[serde(untagged)]
    Unknown(String),
}

/// Available renderers. Vendor keys in persisted metadata are independent of
/// these features, so trimming a renderer cannot erase stored metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Dialect {
    #[cfg(feature = "claude")]
    Claude,
    #[cfg(feature = "openai")]
    Openai,
    #[cfg(feature = "gemini")]
    Gemini,
    #[cfg(feature = "chat")]
    OpenaiChat,
}

impl Dialect {
    pub fn vendor(self) -> Vendor {
        match self {
            #[cfg(feature = "claude")]
            Self::Claude => Vendor::Claude,
            #[cfg(feature = "openai")]
            Self::Openai => Vendor::Openai,
            #[cfg(feature = "gemini")]
            Self::Gemini => Vendor::Gemini,
            #[cfg(feature = "chat")]
            Self::OpenaiChat => Vendor::Openai,
        }
    }
    pub fn wire(self) -> gproxy_protocol::Dialect {
        match self {
            #[cfg(feature = "claude")]
            Self::Claude => gproxy_protocol::Dialect::Claude,
            #[cfg(feature = "openai")]
            Self::Openai => gproxy_protocol::Dialect::OpenAi,
            #[cfg(feature = "gemini")]
            Self::Gemini => gproxy_protocol::Dialect::Gemini,
            #[cfg(feature = "chat")]
            Self::OpenaiChat => gproxy_protocol::Dialect::OpenAiChat,
        }
    }
}

pub type ProviderMetadata = BTreeMap<Vendor, gproxy_protocol::Rest>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Model {
    pub id: String,
    pub dialect: Dialect,
}
