//! Persist call timing independently of optional capture retention.
use crate::entity::usage::usage_record;
use gproxy_seaorm::{
    SchemaProbeExt,
    sea_orm_migration::{MigrationName, MigrationTrait, SchemaManager},
};
use sea_orm::{DbErr, EntityName, Schema, sea_query::Table};

pub struct Migration;
impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261008_000001_usage_timing"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let table = usage_record::Entity.table_name();
        let definition = Schema::new(manager.get_database_backend())
            .create_table_from_entity(usage_record::Entity);
        for name in ["duration_ms", "ttft_ms"] {
            if manager.probe_column(table, name).await? {
                continue;
            }
            let column = definition
                .get_columns()
                .iter()
                .find(|column| column.get_column_name() == name)
                .ok_or_else(|| {
                    DbErr::Migration(format!("usage_records.{name} missing from entity"))
                })?;
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
