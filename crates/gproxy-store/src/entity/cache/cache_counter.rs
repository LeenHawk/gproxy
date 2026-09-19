//! Fixed-window counters; the counter domain of the cache contract.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "cache_counters")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub key: String,
    pub value: i64,
    pub generation: Vec<u8>,
    #[sea_orm(indexed)]
    pub expires_at_ms: i64,
}

impl ActiveModelBehavior for ActiveModel {}
