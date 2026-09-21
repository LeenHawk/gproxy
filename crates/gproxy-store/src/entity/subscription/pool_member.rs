//! One real upstream subscription contributes to one pool through a credential.
//! A canonical source_key deduplicates the same subscription imported under
//! several credentials/providers. Membership is not a change of credential owner.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "subscription_pool_members")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub pool_id: String,
    #[sea_orm(unique)]
    pub credential_id: String,
    /// Canonical issuer + upstream subscription identity, not a local credential
    /// ID. Its construction belongs to the channel. One source backs one pool;
    /// splitting that capacity happens through downstream subscription quotas.
    #[sea_orm(unique)]
    pub source_key: String,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    #[sea_orm(belongs_to, from = "pool_id", to = "id", on_delete = "Cascade")]
    pub pool: BelongsTo<super::pool::Entity>,
    #[sea_orm(belongs_to, from = "credential_id", to = "id", on_delete = "Cascade")]
    pub credential: BelongsTo<crate::entity::upstream::credential::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
