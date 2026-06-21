//! Hrana typed-value row decode helpers.
//!
//! Each returned cell is an internally-tagged object:
//! - integer: `{"type":"integer","value":"123"}` (value is a QUOTED string)
//! - text:    `{"type":"text","value":"..."}`
//! - blob:    `{"type":"blob","base64":"..."}`
//! - null:    `{"type":"null"}`
//!
//! Decoders take a row (`&[Value]`) plus a column index, or a single cell.

use serde_json::Value;

use crate::store::cache::b64;

/// A decoded row: the typed-value cells in column order.
pub type Row = Vec<Value>;

fn cell(row: &[Value], idx: usize) -> anyhow::Result<&Value> {
    row.get(idx)
        .ok_or_else(|| anyhow::anyhow!("libsql row: column index {idx} out of range"))
}

fn cell_type(v: &Value) -> Option<&str> {
    v.get("type").and_then(|t| t.as_str())
}

fn is_null(v: &Value) -> bool {
    matches!(cell_type(v), Some("null") | None)
}

/// Decode an optional i64 cell. `integer`/`text` parse the quoted string value.
pub fn col_opt_i64(row: &[Value], idx: usize) -> anyhow::Result<Option<i64>> {
    let v = cell(row, idx)?;
    if is_null(v) {
        return Ok(None);
    }
    let s = v
        .get("value")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow::anyhow!("libsql col {idx}: expected integer value"))?;
    Ok(Some(s.parse::<i64>()?))
}

/// Decode a non-null i64 cell.
pub fn col_i64(row: &[Value], idx: usize) -> anyhow::Result<i64> {
    col_opt_i64(row, idx)?.ok_or_else(|| anyhow::anyhow!("libsql col {idx}: unexpected NULL i64"))
}

/// Decode an optional text cell.
pub fn col_opt_str(row: &[Value], idx: usize) -> anyhow::Result<Option<String>> {
    let v = cell(row, idx)?;
    if is_null(v) {
        return Ok(None);
    }
    let s = v
        .get("value")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow::anyhow!("libsql col {idx}: expected text value"))?;
    Ok(Some(s.to_owned()))
}

/// Decode a non-null text cell.
pub fn col_str(row: &[Value], idx: usize) -> anyhow::Result<String> {
    col_opt_str(row, idx)?.ok_or_else(|| anyhow::anyhow!("libsql col {idx}: unexpected NULL text"))
}

/// Decode a boolean cell stored as INTEGER 0/1.
pub fn col_bool(row: &[Value], idx: usize) -> anyhow::Result<bool> {
    Ok(col_i64(row, idx)? != 0)
}

/// Decode an optional boolean cell stored as INTEGER 0/1.
pub fn col_opt_bool(row: &[Value], idx: usize) -> anyhow::Result<Option<bool>> {
    Ok(col_opt_i64(row, idx)?.map(|n| n != 0))
}

/// Decode a non-null blob cell.
pub fn col_blob(row: &[Value], idx: usize) -> anyhow::Result<Vec<u8>> {
    col_opt_blob(row, idx)?.ok_or_else(|| anyhow::anyhow!("libsql col {idx}: unexpected NULL blob"))
}

/// Decode an optional blob cell.
pub fn col_opt_blob(row: &[Value], idx: usize) -> anyhow::Result<Option<Vec<u8>>> {
    let v = cell(row, idx)?;
    match cell_type(v) {
        Some("null") | None => Ok(None),
        Some("blob") => {
            let s = v
                .get("base64")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow::anyhow!("libsql col {idx}: blob missing base64"))?;
            Ok(Some(
                b64::decode(s).map_err(|e| anyhow::anyhow!("base64: {e}"))?,
            ))
        }
        // TEXT-stored bytes (defensive): treat the UTF-8 value as raw bytes.
        Some("text") => Ok(v
            .get("value")
            .and_then(|x| x.as_str())
            .map(|s| s.as_bytes().to_vec())),
        Some(other) => anyhow::bail!("libsql col {idx}: expected blob, got {other}"),
    }
}

/// Decode a non-null JSON cell stored as TEXT.
pub fn col_json(row: &[Value], idx: usize) -> anyhow::Result<Value> {
    let s = col_str(row, idx)?;
    Ok(serde_json::from_str(&s)?)
}

/// Decode an optional JSON cell stored as TEXT.
pub fn col_opt_json(row: &[Value], idx: usize) -> anyhow::Result<Option<Value>> {
    match col_opt_str(row, idx)? {
        Some(s) => Ok(Some(serde_json::from_str(&s)?)),
        None => Ok(None),
    }
}

/// Decode a non-null decimal cell stored as a TEXT string.
pub fn col_decimal(row: &[Value], idx: usize) -> anyhow::Result<rust_decimal::Decimal> {
    Ok(col_str(row, idx)?.parse::<rust_decimal::Decimal>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row() -> Row {
        vec![
            json!({"type":"integer","value":"42"}),
            json!({"type":"text","value":"hi"}),
            json!({"type":"null"}),
            json!({"type":"integer","value":"1"}),
            json!({"type":"blob","base64":"aGVsbG8="}),
        ]
    }

    #[test]
    fn decodes_quoted_integer_and_text() {
        let r = row();
        assert_eq!(col_i64(&r, 0).unwrap(), 42);
        assert_eq!(col_str(&r, 1).unwrap(), "hi");
        assert_eq!(col_opt_i64(&r, 2).unwrap(), None);
        assert_eq!(col_opt_str(&r, 2).unwrap(), None);
        assert!(col_bool(&r, 3).unwrap());
        assert_eq!(col_blob(&r, 4).unwrap(), b"hello");
    }

    #[test]
    fn json_roundtrip() {
        let r = vec![json!({"type":"text","value":"{\"a\":1}"})];
        assert_eq!(col_json(&r, 0).unwrap(), json!({"a":1}));
    }
}
