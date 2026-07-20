//! Typed views over opaque per-provider channel settings.

use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// Settings shared by request-shaping implementations across bulletins.
///
/// Unknown fields remain private to their bulletin. Missing fields retain the
/// historical opt-in behavior through serde defaults.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RequestShapeSettings {
    #[serde(deserialize_with = "bool_or_default")]
    pub enable_magic_cache: bool,
    #[serde(deserialize_with = "bool_or_default")]
    pub enable_claude_fable_fallback: bool,
}

impl RequestShapeSettings {
    pub fn from_value(value: &Value) -> Self {
        Self::deserialize(value).unwrap_or_default()
    }
}

fn bool_or_default<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Value::deserialize(deserializer)?.as_bool().unwrap_or(false))
}
