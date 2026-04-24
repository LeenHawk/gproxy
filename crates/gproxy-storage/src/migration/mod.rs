//! Schema migrations applied before entity-based schema sync.
//!
//! This migrator tracks applied migrations in the `seaql_migrations` table
//! (created automatically by sea-orm-migration) and runs any pending migration
//! once per database. Safe to call on every startup.

use sea_orm_migration::prelude::*;

mod m20260416_000001_rename_dispatch_to_routing;
mod m20260417_000001_drop_models_alias_of;
mod m20260420_000001_strip_realtime_routing_rules;
mod m20260424_000001_add_update_channel;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260416_000001_rename_dispatch_to_routing::Migration),
            Box::new(m20260417_000001_drop_models_alias_of::Migration),
            Box::new(m20260420_000001_strip_realtime_routing_rules::Migration),
            Box::new(m20260424_000001_add_update_channel::Migration),
        ]
    }
}
