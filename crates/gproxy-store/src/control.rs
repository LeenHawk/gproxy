//! Coherent persisted control data. No channels, clients, decrypted secrets or regex objects.
//!
//! The rows are split by consumer: [`ControlData`] is what the execution engine
//! assembles a snapshot from, [`RoutingData`] is model exposure and selection,
//! [`IdentityData`] is callers and their entitlements. Each set loads in one
//! batch; [`AllData`] loads all three in a single batch so a host that owns
//! more than the engine assembles every layer from one consistent read.
use crate::{Result, Store, StoreError, entity};
use gproxy_seaorm::{BatchConnectionTrait, BatchQuery, SelectProjection};
use sea_orm::{
    DbBackend, EntityTrait, FromQueryResult, Iterable, PrimaryKeyToColumn, QueryOrder, QueryResult,
};

/// Everything core reads to assemble one execution snapshot.
#[derive(Clone, Debug, Default)]
pub struct ControlData {
    pub settings: Option<entity::config::setting::Model>,
    pub connection_profiles: Vec<entity::config::connection_profile::Model>,
    pub providers: Vec<entity::upstream::provider::Model>,
    pub credentials: Vec<entity::upstream::credential::Model>,
    pub models: Vec<entity::upstream::model::Model>,
    pub provider_models: Vec<entity::upstream::provider_model::Model>,
    pub operation_rules: Vec<entity::upstream::operation_rule::Model>,
    pub operation_endpoints: Vec<entity::upstream::operation_endpoint::Model>,
    pub rewrite_rule_sets: Vec<entity::upstream::rewrite_rule_set::Model>,
    pub rewrite_rules: Vec<entity::upstream::rewrite_rule::Model>,
    pub provider_rewrite_rule_sets: Vec<entity::upstream::provider_rewrite_rule_set::Model>,
    /// Caller budgets and operator limits on credentials share this table.
    pub quotas: Vec<entity::limits::quota::Model>,
    pub price_rules: Vec<entity::pricing::price_rule::Model>,
    pub price_rates: Vec<entity::pricing::price_rate::Model>,
    pub price_tiers: Vec<entity::pricing::price_tier::Model>,
    /// Availability blocks, including expired ones; core filters by until_ms.
    pub credential_blocks: Vec<entity::limits::credential_block::Model>,
}

/// Public model routes and their provider/model members. Core does not
/// route; the layer above resolves a request to providers with these rows.
#[derive(Clone, Debug, Default)]
pub struct RoutingData {
    pub routes: Vec<entity::routing::route::Model>,
    pub route_members: Vec<entity::routing::route_member::Model>,
}

/// Callers and what they are entitled to. Owned by the application layer,
/// never by core.
#[derive(Clone, Debug, Default)]
pub struct IdentityData {
    pub users: Vec<entity::identity::user::Model>,
    pub api_keys: Vec<entity::identity::api_key::Model>,
    pub organizations: Vec<entity::identity::organization::Model>,
    pub organization_members: Vec<entity::identity::organization_member::Model>,
    pub teams: Vec<entity::identity::team::Model>,
    pub team_members: Vec<entity::identity::team_member::Model>,
    pub permissions: Vec<entity::identity::permission::Model>,
    pub rate_limits: Vec<entity::limits::rate_limit::Model>,
    pub oauth_clients: Vec<entity::oauth::client::Model>,
}

/// The three sets as read by one batch, hence one database snapshot: every
/// layer is assembled from the same `config_revision`.
#[derive(Clone, Debug, Default)]
pub struct AllData {
    pub control: ControlData,
    pub routing: RoutingData,
    pub identity: IdentityData,
}

fn ordered<E: EntityTrait>() -> sea_orm::Select<E> {
    let mut query = E::find();
    for key in E::PrimaryKey::iter() {
        query = query.order_by_asc(key.into_column());
    }
    query
}

type Sets = std::vec::IntoIter<Vec<QueryResult>>;

/// Decode the next result set of a batch as `M`. Sets are consumed in the
/// order their queries were appended, so the query and decode lists must stay
/// in step; a missing set is a driver contract violation, not an empty table.
fn take<M: FromQueryResult>(sets: &mut impl Iterator<Item = Vec<QueryResult>>) -> Result<Vec<M>> {
    sets.next()
        .ok_or(StoreError::UnexpectedResult)?
        .iter()
        .map(|row| M::from_query_result(row, "").map_err(Into::into))
        .collect()
}

fn control_queries(backend: DbBackend) -> Result<Vec<BatchQuery>> {
    Ok(vec![
        entity::config::setting::Entity::find_by_id(entity::config::setting::GLOBAL_SETTINGS_ID)
            .batch_query(backend)?,
        ordered::<entity::config::connection_profile::Entity>().batch_query(backend)?,
        ordered::<entity::upstream::provider::Entity>().batch_query(backend)?,
        ordered::<entity::upstream::credential::Entity>().batch_query(backend)?,
        ordered::<entity::upstream::model::Entity>().batch_query(backend)?,
        ordered::<entity::upstream::provider_model::Entity>().batch_query(backend)?,
        ordered::<entity::upstream::operation_rule::Entity>().batch_query(backend)?,
        ordered::<entity::upstream::operation_endpoint::Entity>().batch_query(backend)?,
        ordered::<entity::upstream::rewrite_rule_set::Entity>().batch_query(backend)?,
        ordered::<entity::upstream::rewrite_rule::Entity>().batch_query(backend)?,
        ordered::<entity::upstream::provider_rewrite_rule_set::Entity>().batch_query(backend)?,
        ordered::<entity::limits::quota::Entity>().batch_query(backend)?,
        ordered::<entity::pricing::price_rule::Entity>().batch_query(backend)?,
        ordered::<entity::pricing::price_rate::Entity>().batch_query(backend)?,
        ordered::<entity::pricing::price_tier::Entity>().batch_query(backend)?,
        ordered::<entity::limits::credential_block::Entity>().batch_query(backend)?,
    ])
}

fn control_data(sets: &mut Sets) -> Result<ControlData> {
    let mut data = ControlData {
        settings: take::<entity::config::setting::Model>(sets)?
            .into_iter()
            .next(),
        connection_profiles: take(sets)?,
        providers: take(sets)?,
        credentials: take(sets)?,
        models: take(sets)?,
        provider_models: take(sets)?,
        operation_rules: take(sets)?,
        operation_endpoints: take(sets)?,
        rewrite_rule_sets: take(sets)?,
        rewrite_rules: take(sets)?,
        provider_rewrite_rule_sets: take(sets)?,
        quotas: take(sets)?,
        price_rules: take(sets)?,
        price_rates: take(sets)?,
        price_tiers: take(sets)?,
        credential_blocks: take(sets)?,
    };
    // Application order, not primary-key order: ties keep a stable identity.
    data.rewrite_rules
        .sort_by(|a, b| (a.sort_order, &a.id).cmp(&(b.sort_order, &b.id)));
    data.provider_rewrite_rule_sets
        .sort_by(|a, b| (a.sort_order, &a.id).cmp(&(b.sort_order, &b.id)));
    Ok(data)
}

fn routing_queries(backend: DbBackend) -> Result<Vec<BatchQuery>> {
    Ok(vec![
        ordered::<entity::routing::route::Entity>().batch_query(backend)?,
        ordered::<entity::routing::route_member::Entity>().batch_query(backend)?,
    ])
}

fn routing_data(sets: &mut Sets) -> Result<RoutingData> {
    Ok(RoutingData {
        routes: take(sets)?,
        route_members: take(sets)?,
    })
}

fn identity_queries(backend: DbBackend) -> Result<Vec<BatchQuery>> {
    Ok(vec![
        ordered::<entity::identity::user::Entity>().batch_query(backend)?,
        ordered::<entity::identity::api_key::Entity>().batch_query(backend)?,
        ordered::<entity::identity::organization::Entity>().batch_query(backend)?,
        ordered::<entity::identity::organization_member::Entity>().batch_query(backend)?,
        ordered::<entity::identity::team::Entity>().batch_query(backend)?,
        ordered::<entity::identity::team_member::Entity>().batch_query(backend)?,
        ordered::<entity::identity::permission::Entity>().batch_query(backend)?,
        ordered::<entity::limits::rate_limit::Entity>().batch_query(backend)?,
        ordered::<entity::oauth::client::Entity>().batch_query(backend)?,
    ])
}

fn identity_data(sets: &mut Sets) -> Result<IdentityData> {
    Ok(IdentityData {
        users: take(sets)?,
        api_keys: take(sets)?,
        organizations: take(sets)?,
        organization_members: take(sets)?,
        teams: take(sets)?,
        team_members: take(sets)?,
        permissions: take(sets)?,
        rate_limits: take(sets)?,
        oauth_clients: take(sets)?,
    })
}

impl<C: BatchConnectionTrait> Store<C> {
    /// The execution subset: settings, connection data, providers, credentials,
    /// models, endpoint and rewrite overrides, quotas, pricing and blocks.
    /// Routing and identity rows are loaded separately.
    pub async fn load_control_data(&self) -> Result<ControlData> {
        let backend = self.db.get_database_backend();
        let mut sets = self
            .db
            .query_batch(&control_queries(backend)?)
            .await?
            .into_iter();
        control_data(&mut sets)
    }

    pub async fn load_routing_data(&self) -> Result<RoutingData> {
        let backend = self.db.get_database_backend();
        let mut sets = self
            .db
            .query_batch(&routing_queries(backend)?)
            .await?
            .into_iter();
        routing_data(&mut sets)
    }

    pub async fn load_identity_data(&self) -> Result<IdentityData> {
        let backend = self.db.get_database_backend();
        let mut sets = self
            .db
            .query_batch(&identity_queries(backend)?)
            .await?
            .into_iter();
        identity_data(&mut sets)
    }

    /// All three sets in one batch, so control, routing and identity always
    /// describe the same durable revision.
    pub async fn load_all_data(&self) -> Result<AllData> {
        let backend = self.db.get_database_backend();
        let mut queries = control_queries(backend)?;
        queries.extend(routing_queries(backend)?);
        queries.extend(identity_queries(backend)?);
        let mut sets = self.db.query_batch(&queries).await?.into_iter();
        Ok(AllData {
            control: control_data(&mut sets)?,
            routing: routing_data(&mut sets)?,
            identity: identity_data(&mut sets)?,
        })
    }
}
