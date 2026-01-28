use sea_orm::entity::prelude::*;
use time::OffsetDateTime;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "global_config")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub config_json: Json,
    pub updated_at: OffsetDateTime,
}

impl ActiveModelBehavior for ActiveModel {}
