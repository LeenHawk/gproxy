//! GPROXY v4 persistence entities, currently a review draft.
//!
//! This crate defines database structure. It does not open a database or perform
//! schema changes during construction. Callers can obtain [`schema`] to inspect
//! or initialize the registered entities through SeaORM.

pub mod entity;

use sea_orm::{DbBackend, Schema, SchemaBuilder};

/// Register the draft entities; SeaORM determines the foreign-key creation order.
pub fn schema(backend: DbBackend) -> SchemaBuilder {
    SchemaBuilder::new(Schema::new(backend))
        .register(entity::upstream::provider::Entity)
        .register(entity::upstream::credential::Entity)
        .register(entity::upstream::model::Entity)
        .register(entity::upstream::provider_model::Entity)
        .register(entity::routing::route::Entity)
        .register(entity::routing::route_member::Entity)
        .register(entity::routing::exposed_model::Entity)
        .register(entity::upstream::operation_rule::Entity)
        .register(entity::identity::user::Entity)
        .register(entity::identity::organization::Entity)
        .register(entity::identity::organization_member::Entity)
        .register(entity::identity::team::Entity)
        .register(entity::identity::team_member::Entity)
        .register(entity::identity::api_key::Entity)
        .register(entity::identity::user_session::Entity)
        .register(entity::identity::permission::Entity)
        .register(entity::limits::rate_limit::Entity)
        .register(entity::limits::quota::Entity)
        .register(entity::limits::quota_window::Entity)
        .register(entity::limits::quota_settlement::Entity)
        .register(entity::limits::credential_quota_cycle::Entity)
        .register(entity::pricing::price_rule::Entity)
        .register(entity::pricing::price_rate::Entity)
        .register(entity::pricing::price_tier::Entity)
        .register(entity::usage::usage_record::Entity)
        .register(entity::usage::capture_record::Entity)
        .register(entity::usage::capture_link::Entity)
        .register(entity::usage::capture_event::Entity)
        .register(entity::resource::file_object::Entity)
        .register(entity::resource::resource_binding::Entity)
        .register(entity::resource::protocol_state::Entity)
        .register(entity::config::setting::Entity)
}
