use sea_orm::entity::prelude::*;
use time::OffsetDateTime;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "downstream_traffic")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub created_at: OffsetDateTime,
    pub provider: String,
    pub provider_id: Option<i64>,
    pub operation: String,
    pub model: Option<String>,
    pub user_id: Option<i64>,
    pub key_id: Option<i64>,
    pub trace_id: Option<String>,
    pub request_method: String,
    pub request_path: String,
    pub request_query: Option<String>,
    pub request_headers: String,
    pub request_body: String,
    pub response_status: i32,
    pub response_headers: String,
    pub response_body: String,
    #[sea_orm(belongs_to, from = "provider_id", to = "id", on_delete = "SetNull")]
    pub provider_ref: HasOne<super::providers::Entity>,
    #[sea_orm(belongs_to, from = "user_id", to = "id", on_delete = "SetNull")]
    pub user: HasOne<super::users::Entity>,
    #[sea_orm(belongs_to, from = "key_id", to = "id", on_delete = "SetNull")]
    pub api_key: HasOne<super::api_keys::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
