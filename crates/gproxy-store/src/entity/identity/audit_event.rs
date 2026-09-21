//! Administrative and portal audit trail: who did what, to which entity, with
//! what outcome. Written by the application layer once per accepted operation.
//!
//! The actor and entity columns are historical references and deliberately have
//! no foreign keys: users, keys and the targets themselves may be deleted later,
//! and the trail has to survive exactly those deletions. `detail` is a redacted
//! request summary produced by the operation layer; secrets never reach here.
//! Rows are append-only and are read newest-first by `created_at_ms`.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "audit_events")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// Acting user, when the operation arrived on a console or portal session.
    #[sea_orm(indexed)]
    pub actor_user_id: Option<String>,
    /// Acting API key, when the operation arrived on a programmatic caller.
    #[sea_orm(indexed)]
    pub actor_api_key_id: Option<String>,
    /// Client address as resolved by the host's trusted-proxy policy.
    pub source_ip: Option<String>,
    /// Operation name, e.g. `providers.create`.
    #[sea_orm(indexed)]
    pub action: String,
    /// Target family and ID, when the action names one, e.g. `provider` + its ID.
    pub entity_kind: Option<String>,
    #[sea_orm(indexed)]
    pub entity_id: Option<String>,
    /// `ok` or `error`. A rejected operation is still audited.
    pub outcome: String,
    /// Redacted request summary; shape is owned by the operation that wrote it.
    pub detail: Json,
    #[sea_orm(indexed)]
    pub created_at_ms: i64,
}

impl ActiveModelBehavior for ActiveModel {}

/// The two values [`Model::outcome`] takes.
pub const OUTCOME_OK: &str = "ok";
pub const OUTCOME_ERROR: &str = "error";
