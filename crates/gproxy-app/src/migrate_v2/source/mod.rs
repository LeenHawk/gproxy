mod control;
mod identity;
pub(super) mod logs;
mod pricing;
mod process;
mod usage;

use std::path::Path;

use tokio_rusqlite::rusqlite::{self, OpenFlags};

use super::model::SourceData;

pub(super) async fn read(path: &Path) -> Result<SourceData, crate::AppError> {
    let connection = tokio_rusqlite::Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .await
    .map_err(error)?;
    connection
        .call(|connection| {
            let mut data = SourceData {
                skipped: super::tables::inspect(connection)?,
                ..SourceData::default()
            };
            control::read(connection, &mut data)?;
            identity::read(connection, &mut data)?;
            process::read(connection, &mut data)?;
            usage::read(connection, &mut data)?;
            logs::inspect(connection, &mut data)?;
            Ok::<SourceData, rusqlite::Error>(data)
        })
        .await
        .map_err(error)
}

pub(super) fn json(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<serde_json::Value> {
    decode(row, index)
}

pub(super) fn optional_json(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<Option<serde_json::Value>> {
    row.get::<_, Option<String>>(index)?
        .map(|value| decode_text(&value, index))
        .transpose()
}

pub(super) fn decimal(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<rust_decimal::Decimal> {
    decode(row, index)
}

pub(super) fn optional_decimal(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<Option<rust_decimal::Decimal>> {
    row.get::<_, Option<String>>(index)?
        .map(|value| decode_text(&value, index))
        .transpose()
}

fn decode<T>(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    decode_text(&row.get::<_, String>(index)?, index)
}

fn decode_text<T>(value: &str, index: usize) -> rusqlite::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    value.parse().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn error(error: impl std::fmt::Display) -> crate::AppError {
    crate::AppError::Migration(format!("could not read the v2 database: {error}"))
}
