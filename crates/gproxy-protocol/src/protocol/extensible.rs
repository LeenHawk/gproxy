//! Cheap deserialization paths for extensible wire unions: "known | unknown
//! string" enums and `type`-tagged block unions with a raw fallback.
//!
//! Protocol schemas model extensible wire strings as
//! `enum X { Known(XKnown), Unknown(String) }`. Deriving that with
//! `#[serde(untagged)]` makes every miss construct — then discard — an
//! `unknown variant …, expected one of …` message listing EVERY variant; for
//! the ~100-variant model-id enums that error formatting dominated streaming
//! CPU profiles (the model field rides in every stream event). The helpers
//! here try the known enum against a plain `&str` with an error type whose
//! `custom()` drops the lazily-built message unformatted, so a miss costs one
//! failed string match and nothing else.

use std::fmt;

use serde::Deserialize;

/// `serde::de::Error` that never materializes its message (`format_args!` is
/// lazy; not formatting it is the whole point).
#[derive(Debug)]
pub(crate) struct Discard;

impl fmt::Display for Discard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unknown variant")
    }
}

impl std::error::Error for Discard {}

impl serde::de::Error for Discard {
    fn custom<T: fmt::Display>(_msg: T) -> Self {
        Discard
    }
}

/// Deserialize a unit-variant enum from its wire string; `None` on a miss,
/// with no error message ever formatted.
pub(crate) fn parse_known<T: serde::de::DeserializeOwned>(s: &str) -> Option<T> {
    use serde::de::IntoDeserializer;
    let de: serde::de::value::StrDeserializer<'_, Discard> = s.into_deserializer();
    T::deserialize(de).ok()
}

/// Manual `Deserialize` for a `type`-tagged block union with a `Raw` catch-all
/// variant.
///
/// Replaces `#[serde(untagged)]` trial deserialization (up to N attempts per
/// block) with one buffered read plus one tag match. Semantics match the old
/// untagged behavior exactly:
/// - unknown / missing `type` falls through to `Raw` (lossless forwarding);
/// - a known tag whose body fails to parse ALSO degrades to `Raw` instead of
///   erroring, so unmodeled shape changes are forwarded unchanged rather than
///   rejected at the proxy.
///
/// The enum must keep `#[serde(untagged)]` on a `Serialize`-only derive; each
/// variant's inner struct still owns its single-variant `type` witness, so
/// serialization is unchanged.
macro_rules! type_tag_union_deserialize {
    ($union:ident { $($wire:literal => $variant:ident),+ $(,)? }) => {
        impl<'de> serde::Deserialize<'de> for $union {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let value = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
                match value.get("type").and_then(serde_json::Value::as_str) {
                    $(Some($wire) => {
                        if let Ok(block) = serde::Deserialize::deserialize(&value) {
                            return Ok(Self::$variant(block));
                        }
                    })+
                    _ => {}
                }
                serde::Deserialize::deserialize(value)
                    .map(Self::Raw)
                    .map_err(serde::de::Error::custom)
            }
        }
    };
}

pub(crate) use type_tag_union_deserialize;

/// Manual `Deserialize` body for a `Known(K) | Unknown(String)` wire enum:
/// one string read, one known-variant match, no untagged buffering, no error
/// construction on the unknown fallback.
pub(crate) fn deserialize_extensible<'de, D, K, O>(
    deserializer: D,
    known: fn(K) -> O,
    unknown: fn(String) -> O,
) -> Result<O, D::Error>
where
    D: serde::Deserializer<'de>,
    K: serde::de::DeserializeOwned,
{
    let s = String::deserialize(deserializer)?;
    Ok(match parse_known::<K>(&s) {
        Some(k) => known(k),
        None => unknown(s),
    })
}
