//! Tables behind `StoreCache`, the database-backed `gproxy_cache::Cache` for
//! hosts without a memory-resident process or a Redis, such as edge isolates.

pub mod cache_counter;
pub mod cache_entry;
pub mod cache_permit;
