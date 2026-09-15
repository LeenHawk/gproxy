//! Opaque persisted protocol continuation state. Version belongs to the existing StateStore CAS contract.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "protocol_states")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub scope: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub key: String,
    pub version: Vec<u8>,
    pub payload: Vec<u8>,
    #[sea_orm(indexed)]
    pub expires_at_ms: Option<i64>,
}

impl ActiveModelBehavior for ActiveModel {}
