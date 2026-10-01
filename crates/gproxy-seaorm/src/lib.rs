//! SeaORM 2 entity queries and atomic writes over a Cloudflare D1 binding or a
//! libSQL/Turso database reached through the Hrana HTTP pipeline.
//!
//! [`BatchConnectionTrait`] unifies ordered atomic writes and named queries on
//! native SeaORM connections and D1. [`Projection`] derives D1 result types.
//! On Workers WASM, `D1Connection` implements `sea_orm::ConnectionTrait`, **not**
//! `TransactionTrait`. Use its `atomic_batch` for transactional writes. A SQL
//! statement affecting zero rows is successful and does not roll back a batch.
//!
//! Default results support named model decoding. For SeaORM's `into_tuple()`
//! and other index-based readers, explicitly use [`Projection::by_index`].
//! This preserves SQL column order despite SeaORM's map-backed proxy rows.

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
#[cfg(all(feature = "group-commit", not(target_arch = "wasm32")))]
pub mod group;
/// D1's bound-parameter limit per SQL statement. A logical batch may contain
/// several statements; they are still submitted as one atomic database batch.
pub const D1_MAX_BIND_PARAMETERS: usize = 100;
pub use sea_orm_migration;
#[cfg(all(feature = "postgres-schema", not(target_arch = "wasm32")))]
mod postgres_schema;
#[cfg(all(feature = "postgres-schema", not(target_arch = "wasm32")))]
pub use postgres_schema::PostgresSchemaConnection;

mod migration_support;
pub mod schema;
pub use migration_support::{D1SchemaManagerExt, SchemaProbeExt};
pub use schema::{
    EntityRegistry, SchemaSyncConnectionTrait, SyncReport, is_engine_table, ledger_table_name,
};

#[cfg(any(target_arch = "wasm32", test, feature = "libsql"))]
mod codec;
#[cfg(feature = "libsql")]
mod libsql;
#[cfg(feature = "libsql")]
pub use libsql::{LibsqlConnection, LibsqlFuture, LibsqlRequest, LibsqlResponse, LibsqlTransport};
#[cfg(target_arch = "wasm32")]
mod connection;
#[cfg(target_arch = "wasm32")]
pub use connection::D1Connection;
#[cfg(target_arch = "wasm32")]
mod migration;

fn error(message: impl Into<String>) -> sea_orm::DbErr {
    sea_orm::DbErr::Custom(message.into())
}

#[cfg(test)]
mod tests;
