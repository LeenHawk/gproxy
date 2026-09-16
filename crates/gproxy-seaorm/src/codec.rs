use std::collections::{BTreeMap, BTreeSet};

use sea_orm::{DbErr, Value};
use serde_json::Value as Json;

use crate::{D1Type, Projection, error};

const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

#[derive(Debug)]
pub(crate) struct DecodedRow {
    pub values: BTreeMap<String, Value>,
}

#[derive(Debug)]
pub(crate) struct DecodedExecution {
    pub rows_affected: u64,
    pub last_insert_id: u64,
}

#[cfg(target_arch = "wasm32")]
impl From<DecodedRow> for sea_orm::ProxyRow {
    fn from(row: DecodedRow) -> Self {
        Self { values: row.values }
    }
}

#[cfg(target_arch = "wasm32")]
impl From<DecodedRow> for sea_orm::QueryResult {
    fn from(row: DecodedRow) -> Self {
        sea_orm::ProxyRow::from(row).into()
    }
}

#[cfg(target_arch = "wasm32")]
impl From<DecodedExecution> for sea_orm::ProxyExecResult {
    fn from(result: DecodedExecution) -> Self {
        Self {
            rows_affected: result.rows_affected,
            last_insert_id: result.last_insert_id,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl From<DecodedExecution> for sea_orm::ExecResult {
    fn from(result: DecodedExecution) -> Self {
        sea_orm::ProxyExecResult::from(result).into()
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum Parameter {
    Null,
    Number(f64),
    Text(String),
    Bytes(Vec<u8>),
}

/// Never convert an unsafe integer to Number or silently change its SQL type.
/// Exact wide integers use explicit decimal text parameters and SQL CASTs.
pub(crate) fn parameter(value: &Value) -> Result<Parameter, DbErr> {
    macro_rules! integer {
        ($value:expr) => {{
            let value =
                i64::try_from($value).map_err(|_| error("integer exceeds D1 signed range"))?;
            if !(-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&value) {
                return Err(error(
                    "unsafe JS integer parameter; bind decimal text with CAST(? AS INTEGER)",
                ));
            }
            Parameter::Number(value as f64)
        }};
    }
    Ok(match value {
        Value::Bool(Some(value)) => Parameter::Number(if *value { 1.0 } else { 0.0 }),
        Value::TinyInt(Some(value)) => integer!(*value),
        Value::SmallInt(Some(value)) => integer!(*value),
        Value::Int(Some(value)) => integer!(*value),
        Value::BigInt(Some(value)) => integer!(*value),
        Value::TinyUnsigned(Some(value)) => integer!(*value),
        Value::SmallUnsigned(Some(value)) => integer!(*value),
        Value::Unsigned(Some(value)) => integer!(*value),
        Value::BigUnsigned(Some(value)) => integer!(*value),
        Value::Float(Some(value)) if value.is_finite() => Parameter::Number(f64::from(*value)),
        Value::Double(Some(value)) if value.is_finite() => Parameter::Number(*value),
        Value::Char(Some(value)) => Parameter::Text(value.to_string()),
        Value::String(Some(value)) => Parameter::Text(value.clone()),
        Value::Bytes(Some(value)) => Parameter::Bytes(value.clone()),
        Value::Json(Some(value)) => Parameter::Text(value.to_string()),
        Value::Bool(None)
        | Value::TinyInt(None)
        | Value::SmallInt(None)
        | Value::Int(None)
        | Value::BigInt(None)
        | Value::TinyUnsigned(None)
        | Value::SmallUnsigned(None)
        | Value::Unsigned(None)
        | Value::BigUnsigned(None)
        | Value::Float(None)
        | Value::Double(None)
        | Value::Char(None)
        | Value::String(None)
        | Value::Bytes(None)
        | Value::Json(None) => Parameter::Null,
        _ => return Err(error("unsupported or non-finite D1 parameter")),
    })
}

fn integer(value: &Json) -> Result<i64, DbErr> {
    // Explicit CAST(column AS TEXT) projections preserve all signed 64 bits.
    if let Some(text) = value.as_str() {
        return text
            .parse()
            .map_err(|_| error("invalid exact integer text"));
    }
    let number = value.as_f64().ok_or_else(|| error("expected D1 integer"))?;
    if !number.is_finite() || number.fract() != 0.0 || number.abs() > MAX_SAFE_INTEGER as f64 {
        return Err(error("unsafe JS integer result; use CAST(column AS TEXT)"));
    }
    Ok(number as i64)
}

fn cell(value: &Json, kind: D1Type) -> Result<Value, DbErr> {
    macro_rules! int {
        ($variant:ident, $type:ty) => {
            Value::$variant(Some(
                <$type>::try_from(integer(value)?)
                    .map_err(|_| error("D1 integer out of column range"))?,
            ))
        };
    }
    if value.is_null() {
        return Ok(match kind {
            D1Type::Bool => Value::Bool(None),
            D1Type::I8 => Value::TinyInt(None),
            D1Type::I16 => Value::SmallInt(None),
            D1Type::I32 => Value::Int(None),
            D1Type::I64 => Value::BigInt(None),
            D1Type::U8 => Value::TinyUnsigned(None),
            D1Type::U16 => Value::SmallUnsigned(None),
            D1Type::U32 => Value::Unsigned(None),
            D1Type::U64 => Value::BigUnsigned(None),
            D1Type::F32 => Value::Float(None),
            D1Type::F64 => Value::Double(None),
            D1Type::Text => Value::String(None),
            D1Type::Bytes => Value::Bytes(None),
            D1Type::Json => Value::Json(None),
        });
    }
    Ok(match kind {
        D1Type::Bool => Value::Bool(Some(match integer(value)? {
            0 => false,
            1 => true,
            _ => return Err(error("invalid D1 boolean")),
        })),
        D1Type::I8 => int!(TinyInt, i8),
        D1Type::I16 => int!(SmallInt, i16),
        D1Type::I32 => int!(Int, i32),
        D1Type::I64 => Value::BigInt(Some(integer(value)?)),
        D1Type::U8 => int!(TinyUnsigned, u8),
        D1Type::U16 => int!(SmallUnsigned, u16),
        D1Type::U32 => int!(Unsigned, u32),
        D1Type::U64 => int!(BigUnsigned, u64),
        D1Type::F32 | D1Type::F64 => {
            let number = value.as_f64().ok_or_else(|| error("expected D1 real"))?;
            if !number.is_finite() || (kind == D1Type::F32 && !(number as f32).is_finite()) {
                return Err(error("D1 real out of column range"));
            }
            if kind == D1Type::F32 {
                Value::Float(Some(number as f32))
            } else {
                Value::Double(Some(number))
            }
        }
        D1Type::Text => Value::String(Some(
            value
                .as_str()
                .ok_or_else(|| error("expected D1 text"))?
                .to_owned(),
        )),
        D1Type::Bytes => Value::Bytes(Some(
            value
                .as_array()
                .ok_or_else(|| error("expected D1 BLOB array"))?
                .iter()
                .map(|byte| {
                    byte.as_u64()
                        .and_then(|byte| u8::try_from(byte).ok())
                        .ok_or_else(|| error("invalid D1 BLOB byte"))
                })
                .collect::<Result<_, _>>()?,
        )),
        D1Type::Json => Value::Json(Some(
            serde_json::from_str(
                value
                    .as_str()
                    .ok_or_else(|| error("expected D1 JSON text"))?,
            )
            .map_err(|_| error("invalid stored JSON"))?,
        )),
    })
}

/// D1 raw({columnNames:true}) preserves ordering and exposes duplicate aliases.
pub(crate) fn rows(raw: &Json, projection: &Projection) -> Result<Vec<DecodedRow>, DbErr> {
    let raw = raw
        .as_array()
        .ok_or_else(|| error("expected D1 raw row array"))?;
    let names = raw
        .first()
        .and_then(Json::as_array)
        .ok_or_else(|| error("missing D1 column names"))?;
    let mut seen = BTreeSet::new();
    let columns = names
        .iter()
        .map(|name| {
            let name = name
                .as_str()
                .ok_or_else(|| error("invalid D1 column name"))?;
            if !seen.insert(name) {
                return Err(error(
                    "duplicate D1 result alias; assign distinct SQL aliases",
                ));
            }
            let column = projection
                .columns
                .get(name)
                .ok_or_else(|| error(format!("unregistered D1 result alias: {name}")))?;
            Ok((name, column))
        })
        .collect::<Result<Vec<_>, DbErr>>()?;
    raw.iter()
        .skip(1)
        .map(|row| {
            let row = row.as_array().ok_or_else(|| error("expected D1 raw row"))?;
            if row.len() != columns.len() {
                return Err(error("D1 row width does not match column names"));
            }
            let mut values = BTreeMap::new();
            for (index, ((name, column), value)) in columns.iter().zip(row).enumerate() {
                if value.is_null() && !column.nullable {
                    return Err(error(format!("unexpected NULL in D1 column: {name}")));
                }
                let name = if projection.positional {
                    format!("{index:020}")
                } else {
                    (*name).to_owned()
                };
                values.insert(name, cell(value, column.kind)?);
            }
            Ok(DecodedRow { values })
        })
        .collect()
}

pub(crate) fn execution(result: &Json) -> Result<DecodedExecution, DbErr> {
    if result["success"].as_bool() != Some(true) {
        return Err(error("D1 operation reported failure"));
    }
    let changes = integer(&result["meta"]["changes"])?;
    let last_id = match result["meta"].get("last_row_id") {
        Some(value) if !value.is_null() => integer(value)?,
        _ => 0,
    };
    Ok(DecodedExecution {
        rows_affected: u64::try_from(changes)
            .map_err(|_| error("negative D1 affected row count"))?,
        last_insert_id: u64::try_from(last_id).map_err(|_| error("negative D1 last row id"))?,
    })
}

/// D1 batch returns named objects rather than raw rows/column metadata.
/// Query aliases must be distinct: duplicate SQL aliases are lost by D1 before
/// decoding and cannot be recovered or validated here. Empty results have no schema.
pub(crate) fn batch_rows(result: &Json, projection: &Projection) -> Result<Vec<DecodedRow>, DbErr> {
    if projection.positional {
        return Err(error("D1 batch results have no positional column metadata"));
    }
    if result["success"].as_bool() != Some(true) {
        return Err(error("D1 query batch step reported failure"));
    }
    let rows = result["results"]
        .as_array()
        .ok_or_else(|| error("missing D1 batch query rows"))?;
    let Some(first) = rows.first() else {
        return Ok(Vec::new());
    };
    let first = first
        .as_object()
        .ok_or_else(|| error("expected D1 batch row object"))?;
    let columns = first
        .keys()
        .map(|name| {
            let column = projection
                .columns
                .get(name)
                .ok_or_else(|| error(format!("unregistered D1 result alias: {name}")))?;
            Ok((name, column))
        })
        .collect::<Result<Vec<_>, DbErr>>()?;
    rows.iter()
        .map(|row| {
            let row = row
                .as_object()
                .ok_or_else(|| error("expected D1 batch row object"))?;
            if row.len() != columns.len() {
                return Err(error("inconsistent D1 batch result columns"));
            }
            let mut values = BTreeMap::new();
            for (name, column) in &columns {
                let value = row
                    .get(*name)
                    .ok_or_else(|| error("missing D1 batch result column"))?;
                if value.is_null() && !column.nullable {
                    return Err(error(format!("unexpected NULL in D1 column: {name}")));
                }
                values.insert((*name).clone(), cell(value, column.kind)?);
            }
            Ok(DecodedRow { values })
        })
        .collect()
}
