//! Many-to-many downstream/upstream causality, independent of transport/session.
//! Only Http/WsTurn records participate. The writer must check endpoint sides.
//! One row per pair: D1-U1, D1-U2 is fanout/retry; D1-U1, D2-U1 is sharing.
//! An edge is neither a new upstream invocation nor a second usage contribution.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "capture_links")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub downstream_id: String,
    #[sea_orm(primary_key, auto_increment = false, indexed)]
    pub upstream_id: String,
    /// Order within this downstream request; retries get a new upstream record.
    /// Parallel calls may share an ordinal; break ties by upstream_id for display.
    pub sequence: i32,
    #[sea_orm(
        belongs_to,
        relation_enum = "Downstream",
        from = "downstream_id",
        to = "id",
        on_delete = "Cascade"
    )]
    pub downstream: BelongsTo<super::capture_record::Entity>,
    #[sea_orm(
        belongs_to,
        relation_enum = "Upstream",
        from = "upstream_id",
        to = "id",
        on_delete = "Cascade"
    )]
    pub upstream: BelongsTo<super::capture_record::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
