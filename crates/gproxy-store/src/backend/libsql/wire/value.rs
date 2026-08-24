use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;

use super::WireValue;
use crate::StoreError;
use crate::backend::libsql::invalid;
use crate::backend::{DbValue, Row};

pub(super) fn decode_row(names: &[String], values: Vec<WireValue>) -> Result<Row, StoreError> {
    if names.len() != values.len() {
        return Err(invalid("result column/value count mismatch"));
    }
    names
        .iter()
        .cloned()
        .zip(values.into_iter().map(decode_value))
        .map(|(name, value)| Ok((name, value?)))
        .collect::<Result<Vec<_>, StoreError>>()
        .map(Row::new)
}

fn decode_value(value: WireValue) -> Result<DbValue, StoreError> {
    Ok(match value {
        WireValue::Null => DbValue::Null,
        WireValue::Integer { value } => DbValue::Integer(
            value
                .parse()
                .map_err(|error| invalid(format!("invalid integer result: {error}")))?,
        ),
        WireValue::Float { value } if value.is_finite() => DbValue::Real(value),
        WireValue::Float { .. } => return Err(invalid("non-finite float result")),
        WireValue::Text { value } => DbValue::Text(value),
        WireValue::Blob { base64 } => DbValue::Blob(
            BASE64
                .decode(base64)
                .map_err(|error| invalid(format!("invalid blob result: {error}")))?,
        ),
    })
}
