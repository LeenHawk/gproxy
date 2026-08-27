use crate::StoreError;
use crate::backend::QueryResult;
use crate::records::{ProviderRuleSetRecord, RoutingRuleRecord, RuleRecord, RuleSetRecord};

pub(super) fn routing_rules(result: QueryResult) -> Result<Vec<RoutingRuleRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(RoutingRuleRecord {
                id: row.i64("id")?,
                provider_id: row.i64("provider_id")?,
                operation: row.text("operation")?.into(),
                kind: row.text("kind")?.into(),
                implementation: row.text("implementation")?.into(),
                dest_operation: row.optional_text("dest_operation")?.map(str::to_owned),
                dest_kind: row.optional_text("dest_kind")?.map(str::to_owned),
                sort_order: row.i64("sort_order")?,
                enabled: row.i64("enabled")? != 0,
                origin: row.text("origin")?.into(),
                created_at: row.i64("created_at")?,
                updated_at: row.i64("updated_at")?,
            })
        })
        .collect()
}

pub(super) fn rule_sets(result: QueryResult) -> Result<Vec<RuleSetRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(RuleSetRecord {
                id: row.i64("id")?,
                name: row.text("name")?.into(),
                description: row.optional_text("description")?.map(str::to_owned),
                enabled: row.i64("enabled")? != 0,
                created_at: row.i64("created_at")?,
                updated_at: row.i64("updated_at")?,
            })
        })
        .collect()
}

pub(super) fn rules(result: QueryResult) -> Result<Vec<RuleRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(RuleRecord {
                id: row.i64("id")?,
                rule_set_id: row.i64("rule_set_id")?,
                kind: row.text("kind")?.into(),
                config: json(row.text("config_json")?, "rule config")?,
                filter_model_pattern: row
                    .optional_text("filter_model_pattern")?
                    .map(str::to_owned),
                filter_operations: row
                    .optional_text("filter_operations_json")?
                    .map(|value| json(value, "filter operations"))
                    .transpose()?
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|error| invalid("filter operations", error))?,
                filter_header_pattern: row
                    .optional_text("filter_header_pattern")?
                    .map(str::to_owned),
                sort_order: row.i64("sort_order")?,
                enabled: row.i64("enabled")? != 0,
                created_at: row.i64("created_at")?,
                updated_at: row.i64("updated_at")?,
            })
        })
        .collect()
}

pub(super) fn provider_rule_sets(
    result: QueryResult,
) -> Result<Vec<ProviderRuleSetRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(ProviderRuleSetRecord {
                id: row.i64("id")?,
                provider_id: row.i64("provider_id")?,
                rule_set_id: row.i64("rule_set_id")?,
                sort_order: row.i64("sort_order")?,
                enabled: row.i64("enabled")? != 0,
                origin: row.text("origin")?.into(),
                created_at: row.i64("created_at")?,
                updated_at: row.i64("updated_at")?,
            })
        })
        .collect()
}

fn json(value: &str, field: &'static str) -> Result<serde_json::Value, StoreError> {
    serde_json::from_str(value).map_err(|error| invalid(field, error))
}

fn invalid(field: &'static str, error: impl std::fmt::Display) -> StoreError {
    StoreError::InvalidData {
        field,
        message: error.to_string(),
    }
}
