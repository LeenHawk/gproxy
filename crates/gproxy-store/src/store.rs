//! Database-backed operations; runtime compilation and cache invalidation stay in core.
use crate::repository::Repository;
use gproxy_seaorm::{BatchConnectionTrait, SchemaSyncConnectionTrait, SyncReport};

pub struct Store<C> {
    pub(crate) db: C,
}
impl<C> Store<C> {
    pub fn new(db: C) -> Self {
        Self { db }
    }
    pub fn connection(&self) -> &C {
        &self.db
    }
    pub fn into_connection(self) -> C {
        self.db
    }
}

impl<C: SchemaSyncConnectionTrait> Store<C> {
    /// Create missing tables or incrementally synchronize the complete entity
    /// registry. Call explicitly during startup before serving requests, with
    /// one schema writer. Does not seed application data or run migrations.
    /// Existing types/data transformations still require versioned migrations.
    pub async fn sync(&self) -> crate::Result<SyncReport> {
        let registry = crate::register_entities(self.db.schema_registry());
        Ok(self.db.sync_schema(registry).await?)
    }
}
macro_rules! repositories {
    ($($name:ident => $entity:path),* $(,)?) => {
        impl<C:BatchConnectionTrait> Store<C> {
            $(pub fn $name(&self)->Repository<'_,C,$entity>{Repository::new(&self.db)})*
        }
    };
}
repositories! {
    connection_profiles => crate::entity::config::connection_profile::Entity,
    api_keys => crate::entity::identity::api_key::Entity,
    organizations => crate::entity::identity::organization::Entity,
    organization_members => crate::entity::identity::organization_member::Entity,
    permissions => crate::entity::identity::permission::Entity,
    teams => crate::entity::identity::team::Entity,
    team_members => crate::entity::identity::team_member::Entity,
    users => crate::entity::identity::user::Entity,
    user_sessions => crate::entity::identity::user_session::Entity,
    credential_blocks => crate::entity::limits::credential_block::Entity,
    credential_quota_cycles => crate::entity::limits::credential_quota_cycle::Entity,
    quotas => crate::entity::limits::quota::Entity,
    quota_settlements => crate::entity::limits::quota_settlement::Entity,
    quota_windows => crate::entity::limits::quota_window::Entity,
    rate_limits => crate::entity::limits::rate_limit::Entity,
    oauth_clients => crate::entity::oauth::client::Entity,
    oauth_codes => crate::entity::oauth::code::Entity,
    oauth_devices => crate::entity::oauth::device::Entity,
    oauth_grants => crate::entity::oauth::grant::Entity,
    oauth_tokens => crate::entity::oauth::token::Entity,
    price_rates => crate::entity::pricing::price_rate::Entity,
    price_rules => crate::entity::pricing::price_rule::Entity,
    price_tiers => crate::entity::pricing::price_tier::Entity,
    agent_assignments => crate::entity::resource::agent_assignment::Entity,
    agent_sessions => crate::entity::resource::agent_session::Entity,
    file_objects => crate::entity::resource::file_object::Entity,
    protocol_states => crate::entity::resource::protocol_state::Entity,
    resource_bindings => crate::entity::resource::resource_binding::Entity,
    exposed_models => crate::entity::routing::exposed_model::Entity,
    routes => crate::entity::routing::route::Entity,
    route_members => crate::entity::routing::route_member::Entity,
    subscription_plans => crate::entity::subscription::plan::Entity,
    subscription_plan_limits => crate::entity::subscription::plan_limit::Entity,
    subscription_pools => crate::entity::subscription::pool::Entity,
    subscription_pool_members => crate::entity::subscription::pool_member::Entity,
    subscriptions => crate::entity::subscription::user_subscription::Entity,
    credentials => crate::entity::upstream::credential::Entity,
    models => crate::entity::upstream::model::Entity,
    operation_rules => crate::entity::upstream::operation_rule::Entity,
    operation_endpoints => crate::entity::upstream::operation_endpoint::Entity,
    providers => crate::entity::upstream::provider::Entity,
    provider_models => crate::entity::upstream::provider_model::Entity,
    provider_rewrite_rule_sets => crate::entity::upstream::provider_rewrite_rule_set::Entity,
    rewrite_rules => crate::entity::upstream::rewrite_rule::Entity,
    rewrite_rule_sets => crate::entity::upstream::rewrite_rule_set::Entity,
    capture_events => crate::entity::usage::capture_event::Entity,
    capture_links => crate::entity::usage::capture_link::Entity,
    capture_records => crate::entity::usage::capture_record::Entity,
    usage_records => crate::entity::usage::usage_record::Entity,
}
