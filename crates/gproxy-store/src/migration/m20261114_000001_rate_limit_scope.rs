//! Rate limits gain a counting scope. Existing rows keep one shared counter.
use crate::entity::limits::rate_limit;
use gproxy_seaorm::{
    SchemaProbeExt,
    sea_orm_migration::{MigrationName, MigrationTrait, SchemaManager},
};
use sea_orm::{DbErr, EntityName, Schema, sea_query::Table};

pub struct Migration;
impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261114_000001_rate_limit_scope"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let table = rate_limit::Entity.table_name();
        if manager.probe_column(table, "scope").await? {
            return Ok(());
        }
        let definition = Schema::new(manager.get_database_backend())
            .create_table_from_entity(rate_limit::Entity);
        let column = definition
            .get_columns()
            .iter()
            .find(|column| column.get_column_name() == "scope")
            .ok_or_else(|| DbErr::Migration("rate_limits.scope missing from entity".into()))?;
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
