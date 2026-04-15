use sea_orm::entity::prelude::*;
use time::OffsetDateTime;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "upstream_requests")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = true)]
    pub trace_id: i64,
    pub downstream_trace_id: Option<i64>,
    pub at: OffsetDateTime,
    pub internal: bool,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub request_method: String,
    pub request_headers_json: Json,
    pub request_url: Option<String>,
    pub request_body: Option<Vec<u8>>,
    pub response_status: Option<i32>,
    pub response_headers_json: Json,
    pub response_body: Option<Vec<u8>>,
    pub initial_latency_ms: Option<i64>,
    pub total_latency_ms: Option<i64>,
    pub created_at: OffsetDateTime,
    #[sea_orm(belongs_to, from = "provider_id", to = "id", on_delete = "SetNull")]
    pub provider: HasOne<super::providers::Entity>,
    #[sea_orm(belongs_to, from = "credential_id", to = "id", on_delete = "SetNull")]
    pub credential: HasOne<super::credentials::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
