//! Optional body/header conditions; old rules remain unconditional.
use crate::entity::upstream::rewrite_rule;
use gproxy_seaorm::{
    SchemaProbeExt,
    sea_orm_migration::{MigrationName, MigrationTrait, SchemaManager},
};
use sea_orm::{DbErr, EntityName, Schema, sea_query::Table};

pub struct Migration;
impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261007_000001_rewrite_conditions"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let table = rewrite_rule::Entity.table_name();
        let definition = Schema::new(manager.get_database_backend())
            .create_table_from_entity(rewrite_rule::Entity);
        for name in ["filter_body", "filter_header"] {
            if manager.probe_column(table, name).await? {
                continue;
            }
            let column = definition
                .get_columns()
                .iter()
                .find(|column| column.get_column_name() == name)
                .ok_or_else(|| {
                    DbErr::Migration(format!("rewrite_rules.{name} missing from entity"))
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
