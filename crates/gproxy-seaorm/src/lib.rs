//! SeaORM 2 entity queries and atomic writes over native connections or a
//! libSQL/Turso database reached through the Hrana HTTP pipeline.
//!
//! [`BatchConnectionTrait`] unifies ordered atomic writes and named queries on
//! native SeaORM connections and libSQL. [`Projection`] derives the result
//! types a libSQL connection decodes remote rows with. `LibsqlConnection`
//! implements `sea_orm::ConnectionTrait`, **not** `TransactionTrait`; use
//! `batch` for transactional writes. A SQL statement affecting zero rows is
//! successful and does not roll back a batch.
//!
//! Default results support named model decoding. For SeaORM's `into_tuple()`
//! and other index-based readers, explicitly use [`Projection::by_index`].
//! This preserves SQL column order despite the map-backed decoded rows.

#![forbid(unsafe_code)]

mod projection;
pub use projection::{D1Type, Projection};
mod select;
pub use select::SelectProjection;
mod fixed_decimal;
pub use fixed_decimal::{FixedDecimal, FixedDecimalError};
mod json;
pub use json::json_array_contains_text;
mod batch;
pub use batch::{BatchConnectionTrait, BatchQuery, BatchResult, BatchStatement};
#[cfg(feature = "group-commit")]
pub mod group;
pub use sea_orm_migration;

mod migration_support;
pub mod schema;
pub use migration_support::{D1SchemaManagerExt, SchemaProbeExt};
pub use schema::{
    EntityRegistry, SchemaSyncConnectionTrait, SyncReport, is_engine_table, ledger_table_name,
};

#[cfg(any(test, feature = "libsql"))]
mod codec;
#[cfg(feature = "libsql")]
mod libsql;
#[cfg(feature = "libsql")]
pub use libsql::{LibsqlConnection, LibsqlFuture, LibsqlRequest, LibsqlResponse, LibsqlTransport};

fn error(message: impl Into<String>) -> sea_orm::DbErr {
    sea_orm::DbErr::Custom(message.into())
}

#[cfg(test)]
mod tests;
