//! Add `global_settings.update_channel` column.
//!
//! Persists the operator's self-update channel preference (e.g. "stable"
//! or "prerelease") so the choice survives restart and is reused by
//! /admin/update/check and /admin/update.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum GlobalSettings {
    Table,
    UpdateChannel,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager.has_table(GlobalSettings::Table.to_string()).await? {
            return Ok(());
        }
        if manager
            .has_column(
                GlobalSettings::Table.to_string(),
                GlobalSettings::UpdateChannel.to_string(),
            )
            .await?
        {
            return Ok(());
        }
        manager
            .alter_table(
                Table::alter()
                    .table(GlobalSettings::Table)
                    .add_column(ColumnDef::new(GlobalSettings::UpdateChannel).text().null())
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
