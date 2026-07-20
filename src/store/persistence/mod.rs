//! Durable storage abstraction.
//!
//! Native impl: `db` (SeaORM). Supports SQLite, PostgreSQL, and MySQL.
//! Edge (wasm32) impl: `libsql` (libSQL/Turso over Hrana HTTP).
//! Domain code calls only trait methods.

#[cfg(all(not(target_arch = "wasm32"), feature = "persist-db"))]
pub mod db;
#[cfg(all(target_arch = "wasm32", feature = "persist-libsql"))]
pub mod libsql;

pub mod batch;
pub mod metrics;
#[cfg(any(
    all(not(target_arch = "wasm32"), feature = "persist-db"),
    all(target_arch = "wasm32", feature = "persist-libsql")
))]
pub mod migrations;
pub mod records;
#[doc(hidden)]
pub mod traits;

/// A unique-constraint violation from an upsert (duplicate name/alias/digest,
/// or a composite-key collision). Backends return this carried inside
/// `anyhow::Error`; the admin HTTP layer downcasts it to map to 409 Conflict
/// instead of a generic 500. Kept in the (wasm-agnostic) store layer so the
/// persistence backends never depend on the HTTP error type.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ConflictError(pub String);

impl ConflictError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

/// Filter + cursor for the usage explorer (B4). All filters optional; `before_id`
/// is the keyset cursor (rows have `id` DESC, so "next page" = id < before_id).
#[derive(Debug, Default, Clone)]
pub struct UsageQuery {
    pub at_from: Option<i64>,
    pub at_to: Option<i64>,
    pub provider_id: Option<i64>,
    pub user_id: Option<i64>,
    pub route_name: Option<String>,
    pub model: Option<String>,
    pub before_id: Option<i64>,
    pub limit: u64,
}

/// Filter + cursor for the downstream request log explorer. Provider, user,
/// and route dimensions are resolved through the usage row sharing the same
/// `request_id`.
#[derive(Debug, Default, Clone)]
pub struct LogQuery {
    pub at_from: Option<i64>,
    pub at_to: Option<i64>,
    pub provider_id: Option<i64>,
    pub user_id: Option<i64>,
    pub route_name: Option<String>,
    pub before_id: Option<i64>,
    pub limit: u64,
}

/// Filters for the admin audit-log explorer.
#[derive(Debug, Default, Clone)]
pub struct AuditLogQuery {
    pub at_from: Option<i64>,
    pub at_to: Option<i64>,
    pub actor_id: Option<i64>,
    pub action: Option<String>,
    pub target: Option<String>,
    pub status: Option<i64>,
    pub source_ip: Option<String>,
}

/// Offset/limit pagination passed to persistence backends after HTTP validation.
#[derive(Debug, Clone, Copy)]
pub struct PageQuery {
    pub offset: u64,
    pub limit: u64,
}

/// A page of records plus the total number matching the same filters.
#[derive(Debug)]
pub struct PageResult<T> {
    pub items: Vec<T>,
    pub total: u64,
}

#[cfg(all(not(target_arch = "wasm32"), feature = "persist-db"))]
pub use db::DbPersistence;
#[cfg(all(target_arch = "wasm32", feature = "persist-libsql"))]
pub use libsql::LibsqlPersistence;
pub use traits::PersistenceBackend;
