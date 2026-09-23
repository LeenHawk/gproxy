//! Entity-first schema, grouped by business domain.
//! Business IDs are caller-assigned strings; global settings uses id = 1.
//! Timestamps use Unix milliseconds.
//! Configuration relations declare database FKs. Historical references are documented at each entity.
//! Decimal fields use exact nine-place FixedDecimal integer storage, with textual D1 transport.

pub mod cache;
pub mod config;
pub mod identity;
pub mod limits;
pub mod oauth;
pub mod pricing;
pub mod resource;
pub mod routing;
pub mod upstream;
pub mod usage;
