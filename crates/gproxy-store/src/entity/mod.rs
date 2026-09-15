//! Entity-first review draft, grouped by business domain.
//! Business IDs are caller-assigned strings; global settings uses id = 1.
//! Timestamps use Unix milliseconds.
//! Configuration relations declare database FKs. Historical references are documented at each entity.
//! Decimal fields express the proposed business type; their D1 mapping is not implemented here.

pub mod config;
pub mod identity;
pub mod limits;
pub mod pricing;
pub mod resource;
pub mod routing;
pub mod upstream;
pub mod usage;
