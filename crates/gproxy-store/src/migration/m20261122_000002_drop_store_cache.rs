//! Drop the tables of the database-backed cache; nothing reads them any more.
use gproxy_seaorm::{
    SchemaProbeExt,
    sea_orm_migration::{MigrationName, MigrationTrait, SchemaManager},
};
use sea_orm::{DbErr, sea_query::Table};

pub struct Migration;
impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261122_000002_drop_store_cache"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // A table's indexes go with it on every backend.
        for table in ["cache_entries", "cache_counters", "cache_permits"] {
            if manager.probe_table(table).await? {
                manager
                    .drop_table(Table::drop().table(table).to_owned())
                    .await?;
            }
        }
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        // The rows were transient cache state; there is nothing to restore.
        Ok(())
    }
}
