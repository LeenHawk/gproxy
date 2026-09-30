//! Global request-header allow-list. Existing instances start with no additions.

use gproxy_seaorm::{
    SchemaProbeExt,
    sea_orm_migration::{MigrationName, MigrationTrait, SchemaManager},
};
use sea_orm::{DbErr, EntityName, Schema, sea_query::Table};

use crate::entity::config::setting;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260930_000001_allowed_headers"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let table = setting::Entity.table_name();
        if manager.probe_column(table, "allowed_headers").await? {
            return Ok(());
        }
        let definition =
            Schema::new(manager.get_database_backend()).create_table_from_entity(setting::Entity);
        let column = definition
            .get_columns()
            .iter()
            .find(|column| column.get_column_name() == "allowed_headers")
            .ok_or_else(|| {
                DbErr::Migration("settings.allowed_headers is missing from the entity".into())
            })?;
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
