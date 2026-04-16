//! Schema migrations applied before entity-based schema sync.
//!
//! This migrator tracks applied migrations in the `seaql_migrations` table
//! (created automatically by sea-orm-migration) and runs any pending migration
//! once per database. Safe to call on every startup.

use sea_orm_migration::prelude::*;

mod m20260416_000001_rename_dispatch_to_routing;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(
            m20260416_000001_rename_dispatch_to_routing::Migration,
        )]
    }
}
