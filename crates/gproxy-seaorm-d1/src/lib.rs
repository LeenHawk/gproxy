//! SeaORM 2 entity queries and atomic writes over a Cloudflare D1 binding.
//!
//! [`Projection`] derives result types from an entity or explicit SQL aliases.
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
pub use sea_orm_migration;

mod migration_support;
pub mod schema;
pub use migration_support::D1SchemaManagerExt;

#[cfg(any(target_arch = "wasm32", test))]
mod codec;
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
