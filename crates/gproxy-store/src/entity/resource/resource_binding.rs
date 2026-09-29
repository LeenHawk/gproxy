//! Client/upstream resource mapping, retaining each cloud-agent assignment generation.
//! Normal resources use generation 0 and no assignment. Agent lookups select the
//! session's active generation, never MAX(generation); failed preparations may
//! have newer rows. Old resources keep their original target for history/readback.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "resource_bindings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// Host-derived owner/service scope; agent resources also bind the stable
    /// logical session ID, not the currently selected upstream provider.
    #[sea_orm(unique_key = "public_resource")]
    pub scope: String,
    #[sea_orm(unique_key = "public_resource")]
    pub kind: String,
    #[sea_orm(unique_key = "public_resource", indexed)]
    pub public_id: String,
    /// 0 for ordinary resources, otherwise AgentAssignment.generation.
    #[sea_orm(default_value = 0, unique_key = "public_resource")]
    pub generation: i64,
    #[sea_orm(indexed)]
    pub assignment_id: Option<String>,
    /// Historical target, including after credential/provider configuration deletion.
    pub provider_id: String,
    /// Upstream resource ID; absent for opaque-token mappings stored in secret.
    #[sea_orm(column_type = "Text")]
    pub upstream_id: Option<String>,
    #[sea_orm(indexed)]
    pub user_id: Option<String>,
    #[sea_orm(indexed)]
    pub credential_id: String,
    /// Optional dependency, e.g. task -> environment. Retain IDs as logical history
    /// so resource cleanup cannot cascade-delete dependent remote resources.
    pub parent_binding_id: Option<String>,
    /// Host-sealed upstream remote/session token or other private binding data.
    /// Token-kind public_id stores a digest, never a plaintext downstream bearer.
    pub secret: Option<Vec<u8>>,
    /// Public resource metadata for local list views; excludes tokens/secrets.
    #[sea_orm(default_expr = "sea_orm::sea_query::Expr::cust(\"('{}')\")")]
    pub summary: Json,
    pub file_id: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    #[sea_orm(indexed)]
    pub expires_at_ms: Option<i64>,
    #[sea_orm(belongs_to, from = "user_id", to = "id", on_delete = "Cascade")]
    pub user: BelongsTo<Option<crate::entity::identity::user::Entity>>,
    /// Session purge may clear the assignment pointer, but the generation/target
    /// remain history and must not become an ordinary generation-0 resource.
    #[sea_orm(belongs_to, from = "assignment_id", to = "id", on_delete = "SetNull")]
    pub assignment: BelongsTo<Option<super::agent_assignment::Entity>>,
    #[sea_orm(belongs_to, from = "file_id", to = "id", on_delete = "SetNull")]
    pub file: BelongsTo<Option<super::file_object::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
