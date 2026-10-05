//! Database-backed operations; runtime compilation and cache invalidation stay in core.
use crate::migration::SchemaReport;
use crate::repository::Repository;
use gproxy_seaorm::{BatchConnectionTrait, SchemaSyncConnectionTrait};

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
    /// Prepare a database before serving requests: install a fresh schema or
    /// apply explicit migrations, then add missing entity-defined schema objects.
    /// The migration gate still rejects foreign databases and unknown ledger
    /// versions before any sync DDL. Call with one schema writer at startup.
    pub async fn sync(&self) -> crate::Result<SchemaReport> {
        let report = self.migrate().await?;
        if !report.installed {
            let registry = crate::register_entities(self.db.schema_registry());
            self.db.sync_schema(registry).await?;
        }
        Ok(report)
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
    audit_events => crate::entity::identity::audit_event::Entity,
    organizations => crate::entity::identity::organization::Entity,
    organization_members => crate::entity::identity::organization_member::Entity,
    permissions => crate::entity::identity::permission::Entity,
    teams => crate::entity::identity::team::Entity,
    team_members => crate::entity::identity::team_member::Entity,
    users => crate::entity::identity::user::Entity,
    user_sessions => crate::entity::identity::user_session::Entity,
    credential_blocks => crate::entity::limits::credential_block::Entity,
    credential_quota_cycles => crate::entity::limits::credential_quota_cycle::Entity,
    credential_cycles => crate::entity::limits::credential_cycle::Entity,
    counted_windows => crate::entity::limits::counted_window::Entity,
    cache_entries => crate::entity::cache::cache_entry::Entity,
    cache_counters => crate::entity::cache::cache_counter::Entity,
    cache_permits => crate::entity::cache::cache_permit::Entity,
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
    routes => crate::entity::routing::route::Entity,
    route_members => crate::entity::routing::route_member::Entity,
    credentials => crate::entity::upstream::credential::Entity,
    models => crate::entity::upstream::model::Entity,
    operation_rules => crate::entity::upstream::operation_rule::Entity,
    operation_endpoints => crate::entity::upstream::operation_endpoint::Entity,
    providers => crate::entity::upstream::provider::Entity,
    provider_models => crate::entity::upstream::provider_model::Entity,
    provider_rewrite_rule_sets => crate::entity::upstream::provider_rewrite_rule_set::Entity,
    rewrite_rules => crate::entity::upstream::rewrite_rule::Entity,
    rewrite_rule_sets => crate::entity::upstream::rewrite_rule_set::Entity,
    upstream_records => crate::entity::usage::upstream_record::Entity,
    downstream_records => crate::entity::usage::downstream_record::Entity,
    upstream_events => crate::entity::usage::upstream_event::Entity,
    downstream_events => crate::entity::usage::downstream_event::Entity,
    header_sets => crate::entity::usage::header_set::Entity,
    capture_blobs => crate::entity::usage::capture_blob::Entity,
    capture_bodies => crate::entity::usage::capture_body::Entity,
    capture_body_blobs => crate::entity::usage::capture_body_blob::Entity,
    capture_links => crate::entity::usage::capture_link::Entity,
    usage_records => crate::entity::usage::usage_record::Entity,
}
