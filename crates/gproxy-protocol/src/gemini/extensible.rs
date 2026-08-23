use std::fmt;

use serde::Deserialize;

#[derive(Debug)]
pub(crate) struct Discard;

impl fmt::Display for Discard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("unknown variant")
    }
}

impl std::error::Error for Discard {}

impl serde::de::Error for Discard {
    fn custom<T: fmt::Display>(_: T) -> Self {
        Self
    }
}

pub(crate) fn parse_known<T: serde::de::DeserializeOwned>(value: &str) -> Option<T> {
    use serde::de::IntoDeserializer;

    let deserializer: serde::de::value::StrDeserializer<'_, Discard> = value.into_deserializer();
    T::deserialize(deserializer).ok()
}

pub(crate) fn deserialize_extensible<'de, D, K, O>(
    deserializer: D,
    known: fn(K) -> O,
    unknown: fn(String) -> O,
) -> Result<O, D::Error>
where
    D: serde::Deserializer<'de>,
    K: serde::de::DeserializeOwned,
{
    let value = String::deserialize(deserializer)?;
    Ok(match parse_known::<K>(&value) {
        Some(value) => known(value),
        None => unknown(value),
    })
}
