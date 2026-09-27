//! Small readers shared by every dialect.
//!
//! The usage object of a response is read as a `serde_json::Value`, never
//! through the wire structs in [`crate::wire`]. Those describe a documented
//! request or response exactly, so they require fields upstreams routinely
//! leave out: Chat `usage.total_tokens`, Responses
//! `input_tokens_details.cache_write_tokens`, Claude `server_tool_use`'s two
//! counters, and closed enums for tiers and modalities. A strict parse of a
//! body that omits one of them fails outright, and a failed parse here is an
//! unmetered call. The object is small, so reading it loosely costs nothing;
//! what surrounds it — the content, which can be megabytes — is skipped by the
//! typed envelopes each reader deserializes instead.

use rust_decimal::Decimal;
use serde::Deserialize;
use serde::de::{self, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde_json::Value;
use std::fmt;

use super::types::NormalizedUsage;

/// A non-negative integer count, or `None` when absent or of any other type.
pub(super) fn count(value: &Value, name: &str) -> Option<u64> {
    value.get(name).and_then(Value::as_u64)
}

/// Whether `name` holds a count at all, zero included.
pub(super) fn present(value: &Value, name: &str) -> bool {
    value.get(name).is_some_and(Value::is_u64)
}

/// A decimal given as a JSON number or as a numeric string.
pub(super) fn decimal(value: &Value) -> Option<Decimal> {
    match value {
        Value::Number(number) => number.to_string().parse().ok(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

/// A qualifier reported either as a bare string or as an object that names
/// it: Claude uses `{"name": …}` and OpenAI's tier objects use `{"type": …}`.
pub(super) fn label(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_owned());
    }
    let object = value.as_object()?;
    object
        .get("name")
        .or_else(|| object.get("type"))?
        .as_str()
        .map(str::to_owned)
}

/// Record a quantity as a metric. Zero is left out: a metric's absence is not
/// a measured zero, but a stored zero adds nothing a price could use, and
/// several vendors report every detail counter on every reply.
pub(super) fn metric(usage: &mut NormalizedUsage, name: &str, value: u64) {
    if value > 0 {
        usage.metrics.insert(name.into(), Decimal::from(value));
    }
}

/// Set the tier the upstream actually served as both the pricing dimension
/// and the field service-tier pricing reads.
pub(super) fn service_tier(usage: &mut NormalizedUsage, tier: Option<String>) {
    if let Some(tier) = tier.filter(|tier| !tier.is_empty()) {
        usage.dimensions.insert("service_tier".into(), tier.clone());
        usage.actual_service_tier = Some(tier);
    }
}

/// The pricing dimension that says modality counts are subsets of the token
/// totals rather than additions to them; see `gproxy-core`'s pricing.
pub(super) const MODALITIES_IN_TOTALS: &str = "token_modalities_in_totals";

/// Mark modality metrics, if any were recorded, as subsets of the totals.
pub(super) fn flag_modalities(usage: &mut NormalizedUsage) {
    let modal = usage.metrics.keys().any(|key| {
        [
            "audio_",
            "text_",
            "image_",
            "cached_audio_",
            "cached_image_",
            "cached_text_",
        ]
        .iter()
        .any(|prefix| key.starts_with(prefix))
            && key.ends_with("_tokens")
    });
    if modal {
        usage
            .dimensions
            .insert(MODALITIES_IN_TOTALS.into(), "true".into());
    }
}

/// An object field kept only when it is a JSON object. Anything else — a
/// `null`, a string, a number — reads as absent instead of failing the whole
/// envelope, because a usage object in the wrong shape is no usage, not a
/// reason to discard what the rest of the envelope says.
pub(super) fn object<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Value>, D::Error> {
    Ok(Some(Value::deserialize(deserializer)?).filter(Value::is_object))
}

/// Any JSON value, kept whole. For small qualifiers whose shape varies.
pub(super) fn any<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Ok(Some(Value::deserialize(deserializer)?).filter(|value| !value.is_null()))
}

/// A string field, or `None` for any other JSON type.
pub(super) fn string<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    struct Text;
    impl<'de> Visitor<'de> for Text {
        type Value = Option<String>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("any JSON value")
        }
        fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
            Ok(Some(value.to_owned()))
        }
        fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
            Ok(Some(value))
        }
        fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            while seq.next_element::<IgnoredAny>()?.is_some() {}
            Ok(None)
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
            Ok(None)
        }
    }
    deserializer.deserialize_any(Text)
}

/// The number of elements of an array field, skipping the elements
/// themselves — an image reply's `data` holds base64 images that metering
/// only needs to count. Any other JSON type reads as absent.
pub(super) fn length<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<usize>, D::Error> {
    struct Length;
    impl<'de> Visitor<'de> for Length {
        type Value = Option<usize>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("any JSON value")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut length = 0;
            while seq.next_element::<IgnoredAny>()?.is_some() {
                length += 1;
            }
            Ok(Some(length))
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
            Ok(None)
        }
        fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
    }
    deserializer.deserialize_any(Length)
}
