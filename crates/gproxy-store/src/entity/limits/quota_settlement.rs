//! A request contribution to a quota window. Independent of removable usage-detail records.
//! Subscription quotas use downstream request/turn IDs; pool quotas use actual
//! upstream CaptureRecord IDs, so shared calls are not counted per downstream link.

use gproxy_seaorm::FixedDecimal;
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "quota_settlements")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub window_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub request_id: String,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub amount: FixedDecimal,
    pub settled_at_ms: i64,
    /// Fresh storage-attempt receipt. Duplicate logical request IDs retain their
    /// original receipt, so only newly inserted contributions update the window.
    #[sea_orm(column_type = "Binary(32)")]
    pub receipt: Vec<u8>,
    #[sea_orm(belongs_to, from = "window_id", to = "id", on_delete = "Cascade")]
    pub window: BelongsTo<super::quota_window::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
