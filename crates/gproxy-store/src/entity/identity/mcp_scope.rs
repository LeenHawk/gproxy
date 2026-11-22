//! Rules narrowing which MCP tools a user or an API key may use. A subject
//! with no rule may use every server it has a credential for.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "mcp_scopes")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// Exactly one of `user_id` and `api_key_id` is set.
    #[sea_orm(indexed)]
    pub user_id: Option<String>,
    #[sea_orm(indexed)]
    pub api_key_id: Option<String>,
    /// `mcp:<server>:<tool>`, `mcp:<server>:@read`, `mcp:*`.
    pub scope: String,
    /// `allow` or `deny`.
    pub action: String,
    #[sea_orm(default_value = 0)]
    pub priority: i32,
    #[sea_orm(belongs_to, from = "user_id", to = "id", on_delete = "Cascade")]
    pub user: BelongsTo<Option<super::user::Entity>>,
    #[sea_orm(belongs_to, from = "api_key_id", to = "id", on_delete = "Cascade")]
    pub api_key: BelongsTo<Option<super::api_key::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
