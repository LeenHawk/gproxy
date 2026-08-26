use sea_query::{Alias, OnConflict, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{decimal, insert, json, unsigned, unsigned32, value};
use crate::records::{
    AliasInput, CredentialInput, ExposedModelInput, PriceRateInput, PriceRuleInput, ProviderInput,
    RouteInput, RouteMemberInput, SettingInput,
};

pub(crate) fn insert_provider(input: &ProviderInput) -> Result<Statement, StoreError> {
    insert(
        "providers",
        &[
            "name",
            "channel",
            "settings_json",
            "enabled",
            "tls_fingerprint",
        ],
        vec![
            value(input.name.clone()),
            value(input.channel.clone()),
            value(json(&input.settings, "settings")?),
            value(input.enabled),
            value(
                input
                    .tls_fingerprint
                    .as_ref()
                    .map(|fingerprint| json(fingerprint, "tls_fingerprint"))
                    .transpose()?,
            ),
        ],
    )
}

pub(crate) fn insert_credential(input: &CredentialInput) -> Result<Statement, StoreError> {
    insert(
        "credentials",
        &[
            "provider_id",
            "label",
            "ciphertext",
            "wrapped_key",
            "payload_nonce",
            "key_nonce",
            "version",
            "enabled",
        ],
        vec![
            value(input.provider_id),
            value(input.label.clone()),
            value(input.envelope.ciphertext.clone()),
            value(input.envelope.wrapped_key.clone()),
            value(input.envelope.payload_nonce.clone()),
            value(input.envelope.key_nonce.clone()),
            value(0_i64),
            value(input.enabled),
        ],
    )
}

pub(crate) fn insert_route(input: &RouteInput) -> Result<Statement, StoreError> {
    insert(
        "routes",
        &["name", "max_attempts", "enabled"],
        vec![
            value(input.name.clone()),
            value(unsigned32(input.max_attempts)),
            value(input.enabled),
        ],
    )
}

pub(crate) fn insert_route_member(input: &RouteMemberInput) -> Result<Statement, StoreError> {
    insert(
        "route_members",
        &[
            "route_id",
            "provider_id",
            "credential_id",
            "upstream_model",
            "priority",
            "enabled",
        ],
        vec![
            value(input.route_id),
            value(input.provider_id),
            value(input.credential_id),
            value(input.upstream_model.clone()),
            value(input.priority),
            value(input.enabled),
        ],
    )
}

pub(crate) fn insert_alias(input: &AliasInput) -> Result<Statement, StoreError> {
    insert(
        "aliases",
        &["alias", "target", "provider_id", "priority", "enabled"],
        vec![
            value(input.alias.clone()),
            value(input.target.clone()),
            value(input.provider_id),
            value(input.priority),
            value(input.enabled),
        ],
    )
}

pub(crate) fn insert_exposed_model(input: &ExposedModelInput) -> Result<Statement, StoreError> {
    insert(
        "exposed_models",
        &["name", "route_id", "enabled"],
        vec![
            value(input.name.clone()),
            value(input.route_id),
            value(input.enabled),
        ],
    )
}

pub(crate) fn insert_price_rule(input: &PriceRuleInput) -> Result<Statement, StoreError> {
    crate::records::parse_price_tiers(input.tiers.as_ref())?;
    insert(
        "price_rules",
        &[
            "provider_id",
            "model_pattern",
            "tiers_json",
            "priority",
            "enabled",
        ],
        vec![
            value(input.provider_id),
            value(input.model_pattern.clone()),
            value(
                input
                    .tiers
                    .as_ref()
                    .map(|tiers| json(tiers, "tiers"))
                    .transpose()?,
            ),
            value(input.priority),
            value(input.enabled),
        ],
    )
}

pub(crate) fn insert_price_rate(input: &PriceRateInput) -> Result<Statement, StoreError> {
    insert(
        "price_rates",
        &[
            "rule_id",
            "metric",
            "unit_size",
            "price",
            "conditions_json",
            "priority",
        ],
        vec![
            value(input.rule_id),
            value(input.metric.clone()),
            value(unsigned(input.unit_size, "unit_size")?),
            value(decimal(input.price)),
            value(
                input
                    .conditions
                    .as_ref()
                    .map(|conditions| json(conditions, "conditions"))
                    .transpose()?,
            ),
            value(input.priority),
        ],
    )
}

pub(crate) fn insert_setting(input: &SettingInput) -> Result<Statement, StoreError> {
    let mut query = Query::insert();
    query
        .into_table(Alias::new("settings"))
        .columns([Alias::new("key"), Alias::new("value_json")])
        .values_panic([
            value(input.key.clone()),
            value(json(&input.value, "value")?),
        ])
        .on_conflict(
            OnConflict::column(Alias::new("key"))
                .update_column(Alias::new("value_json"))
                .to_owned(),
        );
    Statement::query(&query)
}
