//! Record the model an upstream response named beside the routed one.
use crate::entity::usage::usage_record;
use gproxy_seaorm::{
    SchemaProbeExt,
    sea_orm_migration::{MigrationName, MigrationTrait, SchemaManager},
};
use sea_orm::{DbErr, EntityName, Schema, sea_query::Table};

pub struct Migration;
impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261102_000001_usage_response_model"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let table = usage_record::Entity.table_name();
        let name = "response_model";
        if manager.probe_column(table, name).await? {
            return Ok(());
        }
        let definition = Schema::new(manager.get_database_backend())
            .create_table_from_entity(usage_record::Entity);
        let column = definition
            .get_columns()
            .iter()
            .find(|column| column.get_column_name() == name)
            .ok_or_else(|| DbErr::Migration(format!("usage_records.{name} missing from entity")))?;
        manager
            .alter_table(
                Table::alter()
                    .table(table)
                    .add_column(column.clone())
                    .to_owned(),
            )
            .await
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
