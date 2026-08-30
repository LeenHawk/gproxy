mod control;
mod identity;
mod process;

use crate::backend::QueryResult;
use crate::query::{control as control_query, identity as identity_query};
use crate::records::ControlSnapshot;
use crate::{Store, StoreError};

impl Store {
    pub async fn control_snapshot(&self) -> Result<ControlSnapshot, StoreError> {
        let statements = vec![
            identity_query::select_organizations()?,
            identity_query::select_teams()?,
            control_query::select_providers()?,
            control_query::select_credential_meta()?,
            control_query::select_routes()?,
            control_query::select_route_members()?,
            control_query::select_aliases()?,
            control_query::select_exposed_models()?,
            control_query::select_provider_models()?,
            identity_query::select_users()?,
            identity_query::select_user_keys()?,
            identity_query::select_permissions()?,
            identity_query::select_rate_limits()?,
            identity_query::select_quotas()?,
            control_query::select_price_rules()?,
            control_query::select_price_rates()?,
            control_query::select_settings()?,
            control_query::select_routing_rules()?,
            control_query::select_rule_sets()?,
            control_query::select_rules()?,
            control_query::select_provider_rule_sets()?,
        ];
        let mut results = self.backend().batch(statements).await?.into_iter();
        Ok(ControlSnapshot {
            organizations: identity::organizations(next(&mut results)?)?,
            teams: identity::teams(next(&mut results)?)?,
            providers: control::providers(next(&mut results)?)?,
            credentials: control::credential_meta(next(&mut results)?)?,
            routes: control::routes(next(&mut results)?)?,
            route_members: control::route_members(next(&mut results)?)?,
            aliases: control::aliases(next(&mut results)?)?,
            exposed_models: control::exposed_models(next(&mut results)?)?,
            provider_models: control::provider_models(next(&mut results)?)?,
            users: identity::users(next(&mut results)?)?,
            user_keys: identity::user_keys(next(&mut results)?)?,
            permissions: identity::permissions(next(&mut results)?)?,
            rate_limits: identity::rate_limits(next(&mut results)?)?,
            quotas: identity::quotas(next(&mut results)?)?,
            price_rules: control::price_rules(next(&mut results)?)?,
            price_rates: control::price_rates(next(&mut results)?)?,
            settings: control::settings(next(&mut results)?)?,
            routing_rules: process::routing_rules(next(&mut results)?)?,
            rule_sets: process::rule_sets(next(&mut results)?)?,
            rules: process::rules(next(&mut results)?)?,
            provider_rule_sets: process::provider_rule_sets(next(&mut results)?)?,
        })
    }
}

fn next(results: &mut impl Iterator<Item = QueryResult>) -> Result<QueryResult, StoreError> {
    results
        .next()
        .ok_or_else(|| StoreError::Database("snapshot query result missing".into()))
}

fn unsigned(value: i64, field: &'static str) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|error| invalid(field, error))
}

fn unsigned32(value: i64, field: &'static str) -> Result<u32, StoreError> {
    u32::try_from(value).map_err(|error| invalid(field, error))
}

fn json(value: &str, field: &'static str) -> Result<serde_json::Value, StoreError> {
    serde_json::from_str(value).map_err(|error| invalid(field, error))
}

fn decimal(value: &str, field: &'static str) -> Result<rust_decimal::Decimal, StoreError> {
    value.parse().map_err(|error| invalid(field, error))
}

fn invalid(field: &'static str, error: impl std::fmt::Display) -> StoreError {
    StoreError::InvalidData {
        field,
        message: error.to_string(),
    }
}
