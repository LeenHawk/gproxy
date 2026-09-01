use serde::de::DeserializeOwned;

use crate::StoreError;
use crate::backend::Row;
use crate::records::{
    CredentialQuotaCycleRecord, QuotaBoundaryConfidence, QuotaBoundarySource, QuotaCoverage,
    QuotaCycleStatus,
};

pub(super) fn parse(row: Row) -> Result<CredentialQuotaCycleRecord, StoreError> {
    Ok(CredentialQuotaCycleRecord {
        id: row.i64("id")?,
        version: u64::try_from(row.i64("version")?).map_err(|error| invalid("version", error))?,
        credential_id: row.i64("credential_id")?,
        window_key: row.text("window_key")?.to_owned(),
        label: row.optional_text("label")?.map(ToOwned::to_owned),
        period_start: row.optional_i64("period_start")?,
        period_end: row.optional_i64("period_end")?,
        boundary_source: enum_value::<QuotaBoundarySource>(&row, "boundary_source")?,
        boundary_confidence: enum_value::<QuotaBoundaryConfidence>(&row, "boundary_confidence")?,
        status: enum_value::<QuotaCycleStatus>(&row, "status")?,
        close_reason: row
            .optional_text("close_reason")?
            .map(|value| deserialize_enum(value, "close_reason"))
            .transpose()?,
        last_observed_at: row.i64("last_observed_at")?,
        upstream_used: decimal(&row, "upstream_used")?,
        upstream_limit: decimal(&row, "upstream_limit")?,
        used_percent: decimal(&row, "used_percent")?,
        coverage: enum_value::<QuotaCoverage>(&row, "coverage")?,
        metrics: serde_json::from_str(row.text("metrics_json")?)
            .map_err(|error| invalid("metrics_json", error))?,
        models: Vec::new(),
    })
}

fn decimal(row: &Row, field: &'static str) -> Result<Option<rust_decimal::Decimal>, StoreError> {
    row.optional_text(field)?
        .map(|value| value.parse().map_err(|error| invalid(field, error)))
        .transpose()
}

fn enum_value<T: DeserializeOwned>(row: &Row, field: &'static str) -> Result<T, StoreError> {
    deserialize_enum(row.text(field)?, field)
}

fn deserialize_enum<T: DeserializeOwned>(
    value: &str,
    field: &'static str,
) -> Result<T, StoreError> {
    serde_json::from_value(serde_json::Value::String(value.to_owned()))
        .map_err(|error| invalid(field, error))
}

fn invalid(field: &'static str, error: impl std::fmt::Display) -> StoreError {
    StoreError::InvalidData {
        field,
        message: error.to_string(),
    }
}
