//! GPROXY v4 entities, batch repositories and atomic persistence operations.
//!
//! Store accepts an existing batch connection. It does not open a database or perform
//! schema changes during construction. Call [`Store::migrate`] explicitly to create the
//! schema or carry an existing one forward; [`schema`] retains one-shot DDL access.
//!
//! Schema changes are versioned. [`migration`] holds the migrator, the baseline
//! that creates the registry on an empty database, and the gate that refuses a
//! database this build does not own rather than altering it and hoping.

pub mod entity;
mod error;
pub use error::{Result, StoreError};
mod repository;
pub use repository::{Key, Page, Repository};
mod store;
pub use store::Store;
mod settings;
pub use gproxy_seaorm::FixedDecimal;
pub use settings::Settings;
mod control;
pub use control::{AllData, ControlData, IdentityData, RoutingData};
mod revision;
pub use revision::Commit;
mod cache;
pub mod migration;
pub mod operations;
pub use cache::StoreCache;
pub use gproxy_seaorm::sea_orm_migration::MigrationStatus;
pub use migration::{MIGRATION_LEDGER, Migrator, SchemaReport, SchemaState};

use sea_orm::{DbBackend, Schema, SchemaBuilder};

/// Register the entities; SeaORM determines the foreign-key creation order.
pub fn schema(backend: DbBackend) -> SchemaBuilder {
    register_entities(SchemaBuilder::new(Schema::new(backend)))
}

fn register_entities<R: gproxy_seaorm::EntityRegistry>(registry: R) -> R {
    registry
        .register(entity::config::connection_profile::Entity)
        .register(entity::upstream::provider::Entity)
        .register(entity::upstream::credential::Entity)
        .register(entity::upstream::model::Entity)
        .register(entity::upstream::provider_model::Entity)
        .register(entity::routing::route::Entity)
        .register(entity::routing::route_member::Entity)
        .register(entity::upstream::operation_rule::Entity)
        .register(entity::upstream::operation_endpoint::Entity)
        .register(entity::upstream::rewrite_rule_set::Entity)
        .register(entity::upstream::rewrite_rule::Entity)
        .register(entity::upstream::provider_rewrite_rule_set::Entity)
        .register(entity::identity::user::Entity)
        .register(entity::identity::organization::Entity)
        .register(entity::identity::organization_member::Entity)
        .register(entity::identity::team::Entity)
        .register(entity::identity::team_member::Entity)
        .register(entity::identity::api_key::Entity)
        .register(entity::identity::user_session::Entity)
        .register(entity::identity::permission::Entity)
        .register(entity::identity::audit_event::Entity)
        .register(entity::oauth::client::Entity)
        .register(entity::oauth::grant::Entity)
        .register(entity::oauth::code::Entity)
        .register(entity::oauth::token::Entity)
        .register(entity::oauth::device::Entity)
        .register(entity::limits::rate_limit::Entity)
        .register(entity::limits::quota::Entity)
        .register(entity::limits::quota_window::Entity)
        .register(entity::limits::quota_settlement::Entity)
        .register(entity::limits::credential_quota_cycle::Entity)
        .register(entity::limits::credential_cycle::Entity)
        .register(entity::limits::credential_block::Entity)
        .register(entity::pricing::price_rule::Entity)
        .register(entity::pricing::price_rate::Entity)
        .register(entity::pricing::price_tier::Entity)
        .register(entity::usage::usage_record::Entity)
        .register(entity::usage::capture_record::Entity)
        .register(entity::usage::capture_link::Entity)
        .register(entity::usage::capture_event::Entity)
        .register(entity::resource::file_object::Entity)
        .register(entity::resource::agent_session::Entity)
        .register(entity::resource::agent_assignment::Entity)
        .register(entity::resource::resource_binding::Entity)
        .register(entity::resource::protocol_state::Entity)
        .register(entity::config::setting::Entity)
        .register(entity::limits::counted_window::Entity)
        .register(entity::cache::cache_entry::Entity)
        .register(entity::cache::cache_counter::Entity)
        .register(entity::cache::cache_permit::Entity)
}
