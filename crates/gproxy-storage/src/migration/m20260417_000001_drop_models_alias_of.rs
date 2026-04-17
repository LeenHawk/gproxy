//! Drop `models.alias_of` column.
//!
//! The `alias_of` indirection has been removed: suffix variants are now
//! regular model rows with their own `model_id` = variant name, and the
//! upstream-side model name is fixed up by rewrite_rules `path:"model"`
//! entries instead of an in-DB pointer. Existing alias rows already have
//! the correct `provider_id` and a variant-named `model_id`, so dropping
//! the column is lossless.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Models {
    Table,
    AliasOf,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager.has_table(Models::Table.to_string()).await? {
            return Ok(());
        }
        if !manager
            .has_column(Models::Table.to_string(), Models::AliasOf.to_string())
            .await?
        {
            return Ok(());
        }
        manager
            .alter_table(
                Table::alter()
                    .table(Models::Table)
                    .drop_column(Models::AliasOf)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // No rollback: the feature no longer exists in the entity model,
        // and the original values are not recoverable from elsewhere.
        Ok(())
    }
}
