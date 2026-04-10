use sea_orm::entity::prelude::*;
use time::OffsetDateTime;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "usages")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = true)]
    pub trace_id: i64,
    pub downstream_trace_id: Option<i64>,
    pub at: OffsetDateTime,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub user_id: Option<i64>,
    pub user_key_id: Option<i64>,
    pub operation: String,
    pub protocol: String,
    pub model: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_input_tokens: Option<i64>,
    pub cache_creation_input_tokens: Option<i64>,
    pub cache_creation_input_tokens_5min: Option<i64>,
    pub cache_creation_input_tokens_1h: Option<i64>,
    /// Per-request quota cost in the same unit as `user_quotas.cost_used`.
    /// Computed by `estimate_billing` at record time and persisted here so
    /// the admin / user dashboards can show how much each call spent
    /// without having to re-price historical rows.
    #[sea_orm(default_value = "0")]
    pub cost: f64,
    pub created_at: OffsetDateTime,
    #[sea_orm(belongs_to, from = "provider_id", to = "id", on_delete = "SetNull")]
    pub provider: HasOne<super::providers::Entity>,
    #[sea_orm(belongs_to, from = "credential_id", to = "id", on_delete = "SetNull")]
    pub credential: HasOne<super::credentials::Entity>,
    #[sea_orm(belongs_to, from = "user_id", to = "id", on_delete = "SetNull")]
    pub user: HasOne<super::users::Entity>,
    #[sea_orm(belongs_to, from = "user_key_id", to = "id", on_delete = "SetNull")]
    pub user_key: HasOne<super::user_keys::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
