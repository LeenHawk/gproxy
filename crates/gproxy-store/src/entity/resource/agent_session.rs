//! Stable logical cloud-agent session, independent of any socket or credential.
//! Reuse the current eligible credential until confirmed quota exhaustion; after
//! switching, keep the new one even when an older credential's quota resets.
//! Routing/model permissions and subscription-pool eligibility still apply.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "agent_sessions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique_key = "session_affinity")]
    pub user_id: String,
    /// Host-defined service/routing namespace, e.g. a Codex remote-control scope.
    #[sea_orm(unique_key = "session_affinity")]
    pub scope: String,
    /// Stable downstream session identity; never a raw bearer token.
    #[sea_orm(unique_key = "session_affinity")]
    pub affinity_key: String,
    /// Original authorization context, retained if configuration is removed.
    /// The host must reauthorize the caller; a missing key is not unrestricted access.
    pub api_key_id: Option<String>,
    /// Fixed downstream USD allocation across credential switches. Historical
    /// reference: removing a subscription must not erase its cloud resources.
    #[sea_orm(indexed)]
    pub subscription_id: Option<String>,
    /// Historical pool selection; no fallback to another pool if it disappears.
    pub pool_id: Option<String>,
    /// Optimistic revision for reservation/activation and all session state writes.
    /// Each assignment uses its reservation revision as a never-reused generation.
    #[sea_orm(default_value = 0)]
    pub version: i64,
    /// Resolve by (session id, generation), not the newest assignment row.
    /// Logical pointer avoids a cyclic creation dependency; activation must
    /// verify the ready assignment and update this pointer atomically by version.
    pub active_generation: Option<i64>,
    #[sea_orm(default_value = "pending")]
    pub state: AgentSessionState,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub expires_at_ms: Option<i64>,
    #[sea_orm(belongs_to, from = "user_id", to = "id", on_delete = "Cascade")]
    pub user: BelongsTo<crate::entity::identity::user::Entity>,
    #[sea_orm(has_many)]
    pub assignments: HasMany<super::agent_assignment::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum AgentSessionState {
    #[sea_orm(string_value = "pending")]
    Pending,
    #[sea_orm(string_value = "ready")]
    Ready,
    #[sea_orm(string_value = "switching")]
    Switching,
    /// No usable assignment or a required resume/rebuild could not complete.
    #[sea_orm(string_value = "blocked")]
    Blocked,
    #[sea_orm(string_value = "closed")]
    Closed,
}
