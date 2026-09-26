//! Credential cycles: the `credential_cycles` table, the observation log's link
//! to it, the log's per-credential time index, and the log's retention setting.
//!
//! Every step is guarded, per the convention in the parent module: on a fresh
//! database the baseline has already created all of it from the registry.

use gproxy_seaorm::{
    SchemaProbeExt,
    sea_orm_migration::{MigrationName, MigrationTrait, SchemaManager},
};
use sea_orm::sea_query::{IndexCreateStatement, Table, TableCreateStatement};
use sea_orm::{DbErr, EntityName, Schema};

use crate::entity::{
    config::setting,
    limits::{credential_cycle, credential_quota_cycle},
};

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260926_000001_credential_cycles"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let schema = Schema::new(manager.get_database_backend());
        let cycles = credential_cycle::Entity.table_name();
        if !manager.probe_table(cycles).await? {
            manager
                .create_table(schema.create_table_from_entity(credential_cycle::Entity))
                .await?;
        }
        create_missing_indexes(
            manager,
            cycles,
            schema.create_index_from_entity(credential_cycle::Entity),
        )
        .await?;

        let log = credential_quota_cycle::Entity.table_name();
        let table = schema.create_table_from_entity(credential_quota_cycle::Entity);
        for column in ["credential_cycle_id", "cycle_cost_usd"] {
            add_missing_column(manager, log, &table, column).await?;
        }
        create_missing_indexes(
            manager,
            log,
            schema.create_index_from_entity(credential_quota_cycle::Entity),
        )
        .await?;

        let settings = setting::Entity.table_name();
        add_missing_column(
            manager,
            settings,
            &schema.create_table_from_entity(setting::Entity),
            "quota_observation_retention_days",
        )
        .await
    }

    /// Additive only; there is nothing to take back that an older build
    /// would trip over.
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

/// Add `column` to `table` exactly as the entity defines it, unless present.
async fn add_missing_column(
    manager: &SchemaManager<'_>,
    table: &str,
    definition: &TableCreateStatement,
    column: &str,
) -> Result<(), DbErr> {
    if manager.probe_column(table, column).await? {
        return Ok(());
    }
    let definition = definition
        .get_columns()
        .iter()
        .find(|candidate| candidate.get_column_name() == column)
        .ok_or_else(|| DbErr::Migration(format!("the entity has no column {table}.{column}")))?;
    manager
        .alter_table(
            Table::alter()
                .table(table.to_owned())
                .add_column(definition.clone())
                .to_owned(),
        )
        .await
}

/// Create each entity index the table does not have yet, by name.
async fn create_missing_indexes(
    manager: &SchemaManager<'_>,
    table: &str,
    indexes: Vec<IndexCreateStatement>,
) -> Result<(), DbErr> {
    for index in indexes {
        let name = index
            .get_index_spec()
            .get_name()
            .ok_or_else(|| DbErr::Migration(format!("an index on {table} has no name")))?
            .to_owned();
        if !manager.probe_index(table, &name).await? {
            manager.create_index(index).await?;
        }
    }
    Ok(())
}
