//! An administrator's word on what one tool of an MCP server does, replacing
//! what the server's own annotations say.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "mcp_tool_natures")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique_key = "provider_tool")]
    pub provider_id: String,
    /// The tool's name as the server lists it.
    #[sea_orm(unique_key = "provider_tool")]
    pub tool: String,
    /// `read`, `write` or `destructive`.
    pub nature: String,
    #[sea_orm(belongs_to, from = "provider_id", to = "id", on_delete = "Cascade")]
    pub provider: BelongsTo<super::provider::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
