//! Consumption core counts itself for a channel-declared Counted quota
//! dimension, per fixed window. The record of what was used; blocks derived
//! from it live in credential_blocks.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "counted_windows")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub credential_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub dimension: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub window_start_ms: i64,
    pub window_end_ms: i64,
    /// Requests or tokens charged so far.
    pub used: i64,
    /// The dimension's declared limit when the window opened.
    #[sea_orm(column_name = "limit_value")]
    pub limit: i64,
}

impl ActiveModelBehavior for ActiveModel {}
