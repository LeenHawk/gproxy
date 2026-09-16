//! One immutable target selection and its handoff outcome for a logical session.
//! Failed/uncertain preparations remain history; never overwrite an old target
//! or replay an uncertain side-effecting create merely because preparation timed out.
//! Credential refresh alone does not create a new assignment generation.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "agent_assignments")]
pub struct Model {
    /// May also serve as an upstream idempotency key where the API supports it.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique_key = "session_generation")]
    pub session_id: String,
    #[sea_orm(unique_key = "session_generation")]
    pub generation: i64,
    /// Previously active generation, not necessarily the last attempted one.
    pub previous_generation: Option<i64>,
    /// Historical target IDs; deleting configuration cannot cascade this history.
    pub provider_id: String,
    #[sea_orm(indexed)]
    pub credential_id: String,
    pub reason: AssignmentReason,
    /// Historical quota observation of the exhausted previous target, when known.
    pub exhausted_cycle_id: Option<String>,
    /// Request/turn that established exhaustion, if it came from an upstream reply.
    pub trigger_request_id: Option<String>,
    #[sea_orm(default_value = "preparing")]
    pub state: AssignmentState,
    pub created_at_ms: i64,
    pub activated_at_ms: Option<i64>,
    pub replaced_at_ms: Option<i64>,
    #[sea_orm(column_type = "Text")]
    pub error: Option<String>,
    #[sea_orm(belongs_to, from = "session_id", to = "id", on_delete = "Cascade")]
    pub session: BelongsTo<super::agent_session::Entity>,
    #[sea_orm(has_many)]
    pub resources: HasMany<super::resource_binding::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(24))")]
pub enum AssignmentReason {
    #[sea_orm(string_value = "initial")]
    Initial,
    /// Confirmed exhaustion for the required model/window; not an arbitrary 429.
    #[sea_orm(string_value = "quota_exhausted")]
    QuotaExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum AssignmentState {
    #[sea_orm(string_value = "preparing")]
    Preparing,
    #[sea_orm(string_value = "active")]
    Active,
    /// Existing calls/resources retain this assignment; no new unbound work.
    #[sea_orm(string_value = "replaced")]
    Replaced,
    #[sea_orm(string_value = "failed")]
    Failed,
    /// Creation may have succeeded upstream; recover its outcome before retrying.
    #[sea_orm(string_value = "uncertain")]
    Uncertain,
}
