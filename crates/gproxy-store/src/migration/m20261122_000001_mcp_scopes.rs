//! MCP scope rules and per-tool nature overrides.
use crate::entity::{identity::mcp_scope, upstream::mcp_tool_nature};
use gproxy_seaorm::{
    SchemaProbeExt,
    sea_orm_migration::{MigrationName, MigrationTrait, SchemaManager},
};
use sea_orm::{DbErr, EntityName, Schema};

pub struct Migration;
impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261122_000001_mcp_scopes"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let schema = Schema::new(manager.get_database_backend());
        macro_rules! create {
            ($entity:ident) => {{
                let table = $entity::Entity.table_name();
                if !manager.probe_table(table).await? {
                    manager
                        .create_table(schema.create_table_from_entity($entity::Entity))
                        .await?;
                }
                for index in schema.create_index_from_entity($entity::Entity) {
                    let name = index.get_index_spec().get_name().unwrap().to_owned();
                    if !manager.probe_index(table, &name).await? {
                        manager.create_index(index).await?;
                    }
                }
            }};
        }
        create!(mcp_scope);
        create!(mcp_tool_nature);
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
