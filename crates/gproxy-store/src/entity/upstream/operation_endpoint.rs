//! Per-provider native-operation URL overrides. This does not remap operations.
//! Missing/disabled entries leave URL construction to provider/channel defaults.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "operation_endpoints")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique_key = "provider_operation_endpoint")]
    pub provider_id: String,
    /// Native operation after protocol adaptation.
    #[sea_orm(unique_key = "provider_operation_endpoint")]
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub operation: String,
    #[sea_orm(unique_key = "provider_operation_endpoint")]
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub dialect: String,
    #[sea_orm(default_value = "http", unique_key = "provider_operation_endpoint")]
    pub transport: EndpointTransport,
    /// Complete method URL; channel-specific path parameters are resolved by
    /// that method. It is not a replacement base URL to append a default path to.
    #[sea_orm(column_type = "Text")]
    pub url: String,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    #[sea_orm(belongs_to, from = "provider_id", to = "id", on_delete = "Cascade")]
    pub provider: BelongsTo<super::provider::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    EnumIter,
    DeriveActiveEnum,
    serde::Serialize,
    serde::Deserialize,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum EndpointTransport {
    #[default]
    #[sea_orm(string_value = "http")]
    Http,
    #[sea_orm(string_value = "websocket")]
    #[serde(rename = "websocket")]
    WebSocket,
}
