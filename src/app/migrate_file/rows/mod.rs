//! MIGRATE-FILE (temporary 2.x bridge, remove in 2.3): tolerant legacy rows.

pub(super) mod authz;
pub(super) mod identity;
pub(super) mod provider;
pub(super) mod routing;
pub(super) mod settings;
pub(super) mod transform;

pub(super) fn default_true() -> bool {
    true
}

pub(super) fn default_weight() -> i64 {
    100
}

pub(super) fn default_json_object() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

pub(super) fn deserialize_decimal<'de, D>(
    deserializer: D,
) -> Result<rust_decimal::Decimal, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use std::str::FromStr as _;

    let value = <serde_json::Value as serde::Deserialize>::deserialize(deserializer)?;
    let text = match value {
        serde_json::Value::String(s) => s,
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => return Ok(rust_decimal::Decimal::ZERO),
        other => {
            return Err(serde::de::Error::custom(format!(
                "invalid decimal: {other}"
            )));
        }
    };
    rust_decimal::Decimal::from_str(&text).map_err(serde::de::Error::custom)
}
