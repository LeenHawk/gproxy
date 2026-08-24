use std::collections::BTreeMap;

use crate::StoreError;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DbValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct QueryResult {
    pub rows: Vec<Row>,
    pub affected_rows: u64,
    pub last_insert_id: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Row(BTreeMap<String, DbValue>);

impl Row {
    pub(crate) fn new(values: impl IntoIterator<Item = (String, DbValue)>) -> Self {
        Self(values.into_iter().collect())
    }

    pub(crate) fn value(&self, name: &'static str) -> Result<&DbValue, StoreError> {
        self.0.get(name).ok_or_else(|| StoreError::InvalidData {
            field: name,
            message: "column missing".to_owned(),
        })
    }

    pub(crate) fn optional_i64(&self, name: &'static str) -> Result<Option<i64>, StoreError> {
        match self.value(name)? {
            DbValue::Null => Ok(None),
            DbValue::Integer(value) => Ok(Some(*value)),
            _ => Err(type_error(name, "integer or null")),
        }
    }

    pub(crate) fn i64(&self, name: &'static str) -> Result<i64, StoreError> {
        self.optional_i64(name)?
            .ok_or_else(|| type_error(name, "integer"))
    }

    pub(crate) fn optional_text(&self, name: &'static str) -> Result<Option<&str>, StoreError> {
        match self.value(name)? {
            DbValue::Null => Ok(None),
            DbValue::Text(value) => Ok(Some(value)),
            _ => Err(type_error(name, "text or null")),
        }
    }

    pub(crate) fn text(&self, name: &'static str) -> Result<&str, StoreError> {
        self.optional_text(name)?
            .ok_or_else(|| type_error(name, "text"))
    }

    pub(crate) fn blob(&self, name: &'static str) -> Result<&[u8], StoreError> {
        match self.value(name)? {
            DbValue::Blob(value) => Ok(value),
            _ => Err(type_error(name, "blob")),
        }
    }

    #[cfg(test)]
    pub(crate) fn entries(&self) -> impl Iterator<Item = (&str, &DbValue)> {
        self.0.iter().map(|(name, value)| (name.as_str(), value))
    }
}

fn type_error(field: &'static str, expected: &'static str) -> StoreError {
    StoreError::InvalidData {
        field,
        message: format!("expected {expected}"),
    }
}
