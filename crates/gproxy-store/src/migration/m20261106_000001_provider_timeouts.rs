//! Nullable provider timeout overrides preserve global inheritance on upgrade.
use crate::entity::upstream::provider;
use gproxy_seaorm::{
    SchemaProbeExt,
    sea_orm_migration::{MigrationName, MigrationTrait, SchemaManager},
};
use sea_orm::{DbErr, EntityName, Schema, sea_query::Table};

pub struct Migration;
impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261106_000001_provider_timeouts"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let table = provider::Entity.table_name();
        let definition =
            Schema::new(manager.get_database_backend()).create_table_from_entity(provider::Entity);
        for name in ["request_timeout_ms", "stream_idle_timeout_ms"] {
            if manager.probe_column(table, name).await? {
                continue;
            }
            let column = definition
                .get_columns()
                .iter()
                .find(|column| column.get_column_name() == name)
                .ok_or_else(|| DbErr::Migration(format!("providers.{name} missing from entity")))?;
            manager
                .alter_table(
                    Table::alter()
                        .table(table)
                        .add_column(column.clone())
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
