//! Persisted availability blocks: the "partially limited" state of a credential.
//! One row per block; core's cache holds the hot copy and Store is authoritative
//! across restarts. Expired rows are pruned lazily by core. credential_id is a
//! configuration FK: deleting the credential drops its blocks.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "credential_blocks")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub credential_id: String,
    /// Channel `QuotaScope` JSON: `"all"`, `{"models":[..]}`, `{"model_prefixes":[..]}`, `"unknown"`.
    pub scope: Json,
    /// Native operation ID; None applies to every operation.
    pub operation: Option<String>,
    #[sea_orm(indexed)]
    pub until_ms: i64,
    /// core `BlockSource` JSON, tagged by `kind`.
    pub source: Json,
    pub observed_at_ms: i64,
    #[sea_orm(belongs_to, from = "credential_id", to = "id", on_delete = "Cascade")]
    pub credential: BelongsTo<crate::entity::upstream::credential::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
