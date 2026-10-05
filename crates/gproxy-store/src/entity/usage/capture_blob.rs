//! Capture payload storage; resolved only through an authorized capture read.
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "capture_blobs")]
pub struct Model {
    /// BLAKE3 content hash keyed by the tenant scope, so equal bytes from two
    /// scopes never share a row and a hash reveals nothing across scopes.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub payload: Vec<u8>,
    pub encoding: String,
    pub raw_size: i64,
}

impl ActiveModelBehavior for ActiveModel {}
