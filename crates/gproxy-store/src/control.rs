//! Coherent persisted control data. No channels, clients, decrypted secrets or regex objects.
use crate::{Result, Store, StoreError, entity};
use gproxy_seaorm::{BatchConnectionTrait, SelectProjection};
use sea_orm::{EntityTrait, FromQueryResult, Iterable, PrimaryKeyToColumn, QueryOrder};

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
    pub routes: Vec<entity::routing::route::Model>,
    pub route_members: Vec<entity::routing::route_member::Model>,
    pub exposed_models: Vec<entity::routing::exposed_model::Model>,
    pub users: Vec<entity::identity::user::Model>,
    pub api_keys: Vec<entity::identity::api_key::Model>,
    pub organizations: Vec<entity::identity::organization::Model>,
    pub organization_members: Vec<entity::identity::organization_member::Model>,
    pub teams: Vec<entity::identity::team::Model>,
    pub team_members: Vec<entity::identity::team_member::Model>,
    pub permissions: Vec<entity::identity::permission::Model>,
    pub quotas: Vec<entity::limits::quota::Model>,
    pub rate_limits: Vec<entity::limits::rate_limit::Model>,
    pub price_rules: Vec<entity::pricing::price_rule::Model>,
    pub price_rates: Vec<entity::pricing::price_rate::Model>,
    pub price_tiers: Vec<entity::pricing::price_tier::Model>,
    pub pools: Vec<entity::subscription::pool::Model>,
    pub pool_members: Vec<entity::subscription::pool_member::Model>,
    pub plans: Vec<entity::subscription::plan::Model>,
    pub plan_limits: Vec<entity::subscription::plan_limit::Model>,
    pub subscriptions: Vec<entity::subscription::user_subscription::Model>,
    pub oauth_clients: Vec<entity::oauth::client::Model>,
    /// Availability blocks, including expired ones; core filters by until_ms.
    pub credential_blocks: Vec<entity::limits::credential_block::Model>,
}

fn ordered<E: EntityTrait>() -> sea_orm::Select<E> {
    let mut query = E::find();
    for key in E::PrimaryKey::iter() {
        query = query.order_by_asc(key.into_column());
    }
    query
}
impl<C: BatchConnectionTrait> Store<C> {
    pub async fn load_control_data(&self) -> Result<ControlData> {
        let backend = self.db.get_database_backend();
        let queries = vec![
            entity::config::setting::Entity::find_by_id(1).batch_query(backend)?,
            ordered::<entity::config::connection_profile::Entity>().batch_query(backend)?,
            ordered::<entity::upstream::provider::Entity>().batch_query(backend)?,
            ordered::<entity::upstream::credential::Entity>().batch_query(backend)?,
            ordered::<entity::upstream::model::Entity>().batch_query(backend)?,
            ordered::<entity::upstream::provider_model::Entity>().batch_query(backend)?,
            ordered::<entity::upstream::operation_rule::Entity>().batch_query(backend)?,
            ordered::<entity::upstream::operation_endpoint::Entity>().batch_query(backend)?,
            ordered::<entity::upstream::rewrite_rule_set::Entity>().batch_query(backend)?,
            ordered::<entity::upstream::rewrite_rule::Entity>().batch_query(backend)?,
            ordered::<entity::upstream::provider_rewrite_rule_set::Entity>()
                .batch_query(backend)?,
            ordered::<entity::routing::route::Entity>().batch_query(backend)?,
            ordered::<entity::routing::route_member::Entity>().batch_query(backend)?,
            ordered::<entity::routing::exposed_model::Entity>().batch_query(backend)?,
            ordered::<entity::identity::user::Entity>().batch_query(backend)?,
            ordered::<entity::identity::api_key::Entity>().batch_query(backend)?,
            ordered::<entity::identity::organization::Entity>().batch_query(backend)?,
            ordered::<entity::identity::organization_member::Entity>().batch_query(backend)?,
            ordered::<entity::identity::team::Entity>().batch_query(backend)?,
            ordered::<entity::identity::team_member::Entity>().batch_query(backend)?,
            ordered::<entity::identity::permission::Entity>().batch_query(backend)?,
            ordered::<entity::limits::quota::Entity>().batch_query(backend)?,
            ordered::<entity::limits::rate_limit::Entity>().batch_query(backend)?,
            ordered::<entity::pricing::price_rule::Entity>().batch_query(backend)?,
            ordered::<entity::pricing::price_rate::Entity>().batch_query(backend)?,
            ordered::<entity::pricing::price_tier::Entity>().batch_query(backend)?,
            ordered::<entity::subscription::pool::Entity>().batch_query(backend)?,
            ordered::<entity::subscription::pool_member::Entity>().batch_query(backend)?,
            ordered::<entity::subscription::plan::Entity>().batch_query(backend)?,
            ordered::<entity::subscription::plan_limit::Entity>().batch_query(backend)?,
            ordered::<entity::subscription::user_subscription::Entity>().batch_query(backend)?,
            ordered::<entity::oauth::client::Entity>().batch_query(backend)?,
            ordered::<entity::limits::credential_block::Entity>().batch_query(backend)?,
        ];
        let mut sets = self.db.query_batch(&queries).await?.into_iter();
        let settings = sets
            .next()
            .ok_or(StoreError::UnexpectedResult)?
            .first()
            .map(|r| entity::config::setting::Model::from_query_result(r, ""))
            .transpose()?;
        let mut data = ControlData {
            settings,
            connection_profiles: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::config::connection_profile::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            providers: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::upstream::provider::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            credentials: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::upstream::credential::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            models: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::upstream::model::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            provider_models: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::upstream::provider_model::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            operation_rules: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::upstream::operation_rule::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            operation_endpoints: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::upstream::operation_endpoint::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            rewrite_rule_sets: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::upstream::rewrite_rule_set::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            rewrite_rules: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::upstream::rewrite_rule::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            provider_rewrite_rule_sets: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| {
                    entity::upstream::provider_rewrite_rule_set::Model::from_query_result(r, "")
                })
                .collect::<std::result::Result<_, _>>()?,
            routes: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::routing::route::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            route_members: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::routing::route_member::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            exposed_models: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::routing::exposed_model::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            users: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::identity::user::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            api_keys: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::identity::api_key::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            organizations: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::identity::organization::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            organization_members: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::identity::organization_member::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            teams: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::identity::team::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            team_members: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::identity::team_member::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            permissions: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::identity::permission::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            quotas: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::limits::quota::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            rate_limits: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::limits::rate_limit::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            price_rules: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::pricing::price_rule::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            price_rates: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::pricing::price_rate::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            price_tiers: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::pricing::price_tier::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            pools: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::subscription::pool::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            pool_members: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::subscription::pool_member::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            plans: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::subscription::plan::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            plan_limits: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::subscription::plan_limit::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            subscriptions: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::subscription::user_subscription::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            oauth_clients: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::oauth::client::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
            credential_blocks: sets
                .next()
                .ok_or(StoreError::UnexpectedResult)?
                .iter()
                .map(|r| entity::limits::credential_block::Model::from_query_result(r, ""))
                .collect::<std::result::Result<_, _>>()?,
        };
        data.rewrite_rules
            .sort_by(|a, b| (a.sort_order, &a.id).cmp(&(b.sort_order, &b.id)));
        data.provider_rewrite_rule_sets
            .sort_by(|a, b| (a.sort_order, &a.id).cmp(&(b.sort_order, &b.id)));
        Ok(data)
    }
}
