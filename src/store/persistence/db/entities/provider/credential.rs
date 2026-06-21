//! `credentials` table SeaORM entity. `secret_json` holds the opaque
//! envelope-encrypted secret as text.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "credentials")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub provider_id: i64,
    pub name: Option<String>,
    pub kind: String,
    #[sea_orm(column_type = "Text")]
    pub secret_json: String,
    pub weight: i64,
    pub rpm_limit: Option<i64>,
    pub tpm_limit: Option<i64>,
    pub proxy_url: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub tls_fingerprint: Option<String>,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
