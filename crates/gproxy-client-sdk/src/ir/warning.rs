use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum WarningKind {
    Unsupported,
    Compatibility,
    Deprecated,
    Other,
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Warning {
    pub kind: WarningKind,
    pub field: String,
    pub message: String,
}

impl Warning {
    pub fn new(kind: WarningKind, field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind,
            field: field.into(),
            message: message.into(),
        }
    }
}
