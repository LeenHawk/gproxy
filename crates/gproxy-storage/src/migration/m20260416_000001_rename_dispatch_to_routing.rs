//! Rename `providers.dispatch_json` column to `providers.routing_json`.
//!
//! The "dispatch table" concept was renamed to "routing table" across the
//! codebase. This migration aligns the stored schema.
//!
//! Safe on fresh databases: `sea-orm-migration` records migration ledger in
//! `seaql_migrations`, so this runs at most once per DB. The entity-based
//! schema sync runs AFTER migrator, so a brand-new DB will have this
//! migration marked applied (with no-op effect) and then the sync will
//! create the `providers` table with the already-correct `routing_json`
//! column.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Providers {
    Table,
    DispatchJson,
    RoutingJson,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // If the providers table hasn't been created yet (fresh DB — sync()
        // will create it), nothing to rename.
        if !manager.has_table(Providers::Table.to_string()).await? {
            return Ok(());
        }
        // If old column isn't present, nothing to do (already migrated, or
        // fresh DB created by entity sync with the new name).
        if !manager
            .has_column(Providers::Table.to_string(), Providers::DispatchJson.to_string())
            .await?
        {
            return Ok(());
        }

        manager
            .alter_table(
                Table::alter()
                    .table(Providers::Table)
                    .rename_column(Providers::DispatchJson, Providers::RoutingJson)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager.has_table(Providers::Table.to_string()).await? {
            return Ok(());
        }
        if !manager
            .has_column(Providers::Table.to_string(), Providers::RoutingJson.to_string())
            .await?
        {
            return Ok(());
        }

        manager
            .alter_table(
                Table::alter()
                    .table(Providers::Table)
                    .rename_column(Providers::RoutingJson, Providers::DispatchJson)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
