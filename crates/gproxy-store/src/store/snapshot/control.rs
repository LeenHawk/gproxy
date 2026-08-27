use super::{decimal, json, unsigned, unsigned32};
use crate::StoreError;
use crate::backend::QueryResult;
use crate::records::{
    AliasRecord, CredentialMetaRecord, ExposedModelRecord, PriceRateRecord, PriceRuleRecord,
    ProviderRecord, RouteMemberRecord, RouteRecord, SettingRecord,
};

pub(super) fn providers(result: QueryResult) -> Result<Vec<ProviderRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(ProviderRecord {
                id: row.i64("id")?,
                name: row.text("name")?.to_owned(),
                label: row.optional_text("label")?.map(str::to_owned),
                channel: row.text("channel")?.to_owned(),
                settings: json(row.text("settings_json")?, "settings_json")?,
                credential_strategy: row.text("credential_strategy")?.to_owned(),
                proxy_url: row.optional_text("proxy_url")?.map(str::to_owned),
                tls_fingerprint: row
                    .optional_text("tls_fingerprint")?
                    .map(|value| json(value, "tls_fingerprint"))
                    .transpose()?,
                enabled: row.i64("enabled")? != 0,
            })
        })
        .collect()
}

pub(super) fn credential_meta(
    result: QueryResult,
) -> Result<Vec<CredentialMetaRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(CredentialMetaRecord {
                id: row.i64("id")?,
                provider_id: row.i64("provider_id")?,
                kind: row.text("kind")?.to_owned(),
                version: unsigned(row.i64("version")?, "credential version")?,
                enabled: row.i64("enabled")? != 0,
                weight: unsigned32(row.i64("weight")?, "credential weight")?,
                rpm_limit: row
                    .optional_i64("rpm_limit")?
                    .map(|value| unsigned32(value, "credential rpm_limit"))
                    .transpose()?,
                tpm_limit: row
                    .optional_i64("tpm_limit")?
                    .map(|value| unsigned(value, "credential tpm_limit"))
                    .transpose()?,
                proxy_url: row.optional_text("proxy_url")?.map(str::to_owned),
                tls_fingerprint: row
                    .optional_text("tls_fingerprint")?
                    .map(|value| json(value, "credential tls_fingerprint"))
                    .transpose()?,
            })
        })
        .collect()
}

pub(super) fn routes(result: QueryResult) -> Result<Vec<RouteRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(RouteRecord {
                id: row.i64("id")?,
                name: row.text("name")?.to_owned(),
                max_attempts: unsigned32(row.i64("max_attempts")?, "max_attempts")?,
                enabled: row.i64("enabled")? != 0,
            })
        })
        .collect()
}

pub(super) fn route_members(result: QueryResult) -> Result<Vec<RouteMemberRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(RouteMemberRecord {
                id: row.i64("id")?,
                route_id: row.i64("route_id")?,
                provider_id: row.i64("provider_id")?,
                credential_id: row.optional_i64("credential_id")?,
                upstream_model: row.text("upstream_model")?.to_owned(),
                tier: unsigned32(row.i64("tier")?, "route member tier")?,
                weight: unsigned32(row.i64("weight")?, "route member weight")?,
                enabled: row.i64("enabled")? != 0,
            })
        })
        .collect()
}

pub(super) fn aliases(result: QueryResult) -> Result<Vec<AliasRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(AliasRecord {
                id: row.i64("id")?,
                alias: row.text("alias")?.to_owned(),
                target: row.text("target")?.to_owned(),
                provider_id: row.optional_i64("provider_id")?,
                priority: row.i64("priority")?,
                enabled: row.i64("enabled")? != 0,
            })
        })
        .collect()
}

pub(super) fn exposed_models(result: QueryResult) -> Result<Vec<ExposedModelRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(ExposedModelRecord {
                id: row.i64("id")?,
                name: row.text("name")?.to_owned(),
                route_id: row.i64("route_id")?,
                display_name: row.optional_text("display_name")?.map(str::to_owned),
                variants: row
                    .optional_text("variants_json")?
                    .map(|value| json(value, "model variants"))
                    .transpose()?,
                context_window: row.optional_i64("context_window")?,
                max_output_tokens: row.optional_i64("max_output_tokens")?,
                thinking_supported: row
                    .optional_i64("thinking_supported")?
                    .map(|value| value != 0),
                thinking_adaptive_supported: row
                    .optional_i64("thinking_adaptive_supported")?
                    .map(|value| value != 0),
                thinking_enabled_supported: row
                    .optional_i64("thinking_enabled_supported")?
                    .map(|value| value != 0),
                enabled: row.i64("enabled")? != 0,
            })
        })
        .collect()
}

pub(super) fn price_rules(result: QueryResult) -> Result<Vec<PriceRuleRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(PriceRuleRecord {
                id: row.i64("id")?,
                provider_id: row.optional_i64("provider_id")?,
                model_pattern: row.text("model_pattern")?.to_owned(),
                tiers: row
                    .optional_text("tiers_json")?
                    .map(|value| json(value, "tiers_json"))
                    .transpose()?,
                priority: row.i64("priority")?,
                enabled: row.i64("enabled")? != 0,
            })
        })
        .collect()
}

pub(super) fn price_rates(result: QueryResult) -> Result<Vec<PriceRateRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(PriceRateRecord {
                id: row.i64("id")?,
                rule_id: row.i64("rule_id")?,
                metric: row.text("metric")?.to_owned(),
                unit_size: unsigned(row.i64("unit_size")?, "unit_size")?,
                price: decimal(row.text("price")?, "price")?,
                conditions: row
                    .optional_text("conditions_json")?
                    .map(|value| json(value, "conditions_json"))
                    .transpose()?,
                priority: row.i64("priority")?,
            })
        })
        .collect()
}

pub(super) fn settings(result: QueryResult) -> Result<Vec<SettingRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(SettingRecord {
                key: row.text("key")?.to_owned(),
                value: json(row.text("value_json")?, "value_json")?,
            })
        })
        .collect()
}
