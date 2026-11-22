//! The provider configuration an upstream kind reads, as data a management UI
//! renders a form from. Serialized the way the host's own DTOs are.

/// How a configuration value is written, for form rendering and validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", ts(rename_all = "snake_case"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ConfigKeyKind {
    String,
    Bool,
    Integer,
    /// Any other JSON shape: an array, or an object that is not headers.
    Json,
    /// HTTP header names, or header name/value pairs.
    HeaderList,
}

/// One entry of a provider's configuration. Unless noted, the name is a key of
/// the provider's `config` JSON; `base_url` is the provider's own column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ConfigKey {
    pub name: &'static str,
    pub kind: ConfigKeyKind,
    /// The provider cannot serve a request without it. Everything else has a
    /// default.
    pub required: bool,
    pub description: &'static str,
    /// Display-only default or URL template; never written into config by the UI.
    pub placeholder: Option<&'static str>,
}

impl ConfigKey {
    pub const fn with_placeholder(mut self, placeholder: &'static str) -> Self {
        self.placeholder = Some(placeholder);
        self
    }

    pub const fn required(
        name: &'static str,
        kind: ConfigKeyKind,
        description: &'static str,
    ) -> Self {
        Self {
            name,
            kind,
            required: true,
            description,
            placeholder: None,
        }
    }
    pub const fn optional(
        name: &'static str,
        kind: ConfigKeyKind,
        description: &'static str,
    ) -> Self {
        Self {
            name,
            kind,
            required: false,
            description,
            placeholder: None,
        }
    }
}
