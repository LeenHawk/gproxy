use sea_query::{Alias, Cond, Expr, ExprTrait, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{insert, json, select_all, update, value};
use crate::records::{ProviderRuleSetInput, RoutingRuleInput, RuleInput, RuleSetInput};

pub(crate) fn select_routing_rules() -> Result<Statement, StoreError> {
    select_all(
        "routing_rules",
        &[
            "id",
            "provider_id",
            "operation",
            "kind",
            "implementation",
            "dest_operation",
            "dest_kind",
            "sort_order",
            "enabled",
            "origin",
            "created_at",
            "updated_at",
        ],
    )
}

pub(crate) fn select_rule_sets() -> Result<Statement, StoreError> {
    select_all(
        "rule_sets",
        &[
            "id",
            "name",
            "description",
            "enabled",
            "created_at",
            "updated_at",
        ],
    )
}

pub(crate) fn select_rules() -> Result<Statement, StoreError> {
    select_all(
        "rules",
        &[
            "id",
            "rule_set_id",
            "kind",
            "config_json",
            "filter_model_pattern",
            "filter_operations_json",
            "filter_header_pattern",
            "sort_order",
            "enabled",
            "created_at",
            "updated_at",
        ],
    )
}

pub(crate) fn select_provider_rule_sets() -> Result<Statement, StoreError> {
    select_all(
        "provider_rule_sets",
        &[
            "id",
            "provider_id",
            "rule_set_id",
            "sort_order",
            "enabled",
            "origin",
            "created_at",
            "updated_at",
        ],
    )
}

pub(crate) fn insert_routing_rule(input: &RoutingRuleInput) -> Result<Statement, StoreError> {
    let now = now();
    insert(
        "routing_rules",
        &[
            "provider_id",
            "operation",
            "kind",
            "implementation",
            "dest_operation",
            "dest_kind",
            "sort_order",
            "enabled",
            "origin",
            "created_at",
            "updated_at",
        ],
        vec![
            value(input.provider_id),
            value(input.operation.clone()),
            value(input.kind.clone()),
            value(input.implementation.clone()),
            value(input.dest_operation.clone()),
            value(input.dest_kind.clone()),
            value(input.sort_order),
            value(input.enabled),
            value(now),
            value(now),
        ],
    )
}

pub(crate) fn update_routing_rule(
    id: i64,
    input: &RoutingRuleInput,
) -> Result<Statement, StoreError> {
    update(
        "routing_rules",
        id,
        &[
            "provider_id",
            "operation",
            "kind",
            "implementation",
            "dest_operation",
            "dest_kind",
            "sort_order",
            "enabled",
            "origin",
            "updated_at",
        ],
        vec![
            value(input.provider_id),
            value(input.operation.clone()),
            value(input.kind.clone()),
            value(input.implementation.clone()),
            value(input.dest_operation.clone()),
            value(input.dest_kind.clone()),
            value(input.sort_order),
            value(input.enabled),
            value("operator"),
            value(now()),
        ],
    )
}

pub(crate) fn insert_routing_default(input: &RoutingRuleInput) -> Result<Statement, StoreError> {
    let mut exists = Query::select();
    exists
        .expr(Expr::val(1))
        .from(Alias::new("routing_rules"))
        .and_where(Expr::col(Alias::new("provider_id")).eq(input.provider_id))
        .and_where(Expr::col(Alias::new("operation")).eq(input.operation.clone()))
        .and_where(Expr::col(Alias::new("kind")).eq(input.kind.clone()));
    let now = now();
    let mut values = Query::select();
    values
        .exprs([
            value(input.provider_id),
            value(input.operation.clone()),
            value(input.kind.clone()),
            value(input.implementation.clone()),
            value(input.dest_operation.clone()),
            value(input.dest_kind.clone()),
            value(input.sort_order),
            value(input.enabled),
            value("channel_default"),
            value(now),
            value(now),
        ])
        .cond_where(Cond::all().not().add(Expr::exists(exists)));
    let mut query = Query::insert();
    query
        .into_table(Alias::new("routing_rules"))
        .columns(
            [
                "provider_id",
                "operation",
                "kind",
                "implementation",
                "dest_operation",
                "dest_kind",
                "sort_order",
                "enabled",
                "origin",
                "created_at",
                "updated_at",
            ]
            .into_iter()
            .map(Alias::new),
        )
        .select_from(values)
        .map_err(|error| StoreError::Database(error.to_string()))?;
    Statement::query(&query)
}

pub(crate) fn delete_provider_routing_rules(provider_id: i64) -> Result<Statement, StoreError> {
    let mut query = Query::delete();
    query
        .from_table(Alias::new("routing_rules"))
        .and_where(Expr::col(Alias::new("provider_id")).eq(provider_id));
    Statement::query(&query)
}

pub(crate) fn insert_rule_set(input: &RuleSetInput) -> Result<Statement, StoreError> {
    let now = now();
    insert(
        "rule_sets",
        &["name", "description", "enabled", "created_at", "updated_at"],
        vec![
            value(input.name.clone()),
            value(input.description.clone()),
            value(input.enabled),
            value(now),
            value(now),
        ],
    )
}

pub(crate) fn update_rule_set(id: i64, input: &RuleSetInput) -> Result<Statement, StoreError> {
    update(
        "rule_sets",
        id,
        &["name", "description", "enabled", "updated_at"],
        vec![
            value(input.name.clone()),
            value(input.description.clone()),
            value(input.enabled),
            value(now()),
        ],
    )
}

pub(crate) fn insert_rule(input: &RuleInput) -> Result<Statement, StoreError> {
    let now = now();
    insert(
        "rules",
        &[
            "rule_set_id",
            "kind",
            "config_json",
            "filter_model_pattern",
            "filter_operations_json",
            "filter_header_pattern",
            "sort_order",
            "enabled",
            "created_at",
            "updated_at",
        ],
        vec![
            value(input.rule_set_id),
            value(input.kind.clone()),
            value(json(&input.config, "rule config")?),
            value(input.filter_model_pattern.clone()),
            value(
                input
                    .filter_operations
                    .as_ref()
                    .map(operations_json)
                    .transpose()?,
            ),
            value(input.filter_header_pattern.clone()),
            value(input.sort_order),
            value(input.enabled),
            value("operator"),
            value(now),
            value(now),
        ],
    )
}

pub(crate) fn update_rule(id: i64, input: &RuleInput) -> Result<Statement, StoreError> {
    update(
        "rules",
        id,
        &[
            "rule_set_id",
            "kind",
            "config_json",
            "filter_model_pattern",
            "filter_operations_json",
            "filter_header_pattern",
            "sort_order",
            "enabled",
            "updated_at",
        ],
        vec![
            value(input.rule_set_id),
            value(input.kind.clone()),
            value(json(&input.config, "rule config")?),
            value(input.filter_model_pattern.clone()),
            value(
                input
                    .filter_operations
                    .as_ref()
                    .map(operations_json)
                    .transpose()?,
            ),
            value(input.filter_header_pattern.clone()),
            value(input.sort_order),
            value(input.enabled),
            value(now()),
        ],
    )
}

pub(crate) fn insert_provider_rule_set_default(
    input: &ProviderRuleSetInput,
) -> Result<Statement, StoreError> {
    let now = now();
    insert(
        "provider_rule_sets",
        &[
            "provider_id",
            "rule_set_id",
            "sort_order",
            "enabled",
            "origin",
            "created_at",
            "updated_at",
        ],
        vec![
            value(input.provider_id),
            value(input.rule_set_id),
            value(input.sort_order),
            value(input.enabled),
            value("channel_default"),
            value(now),
            value(now),
        ],
    )
}

pub(crate) fn insert_provider_rule_set(
    input: &ProviderRuleSetInput,
) -> Result<Statement, StoreError> {
    let now = now();
    insert(
        "provider_rule_sets",
        &[
            "provider_id",
            "rule_set_id",
            "sort_order",
            "enabled",
            "origin",
            "created_at",
            "updated_at",
        ],
        vec![
            value(input.provider_id),
            value(input.rule_set_id),
            value(input.sort_order),
            value(input.enabled),
            value("operator"),
            value(now),
            value(now),
        ],
    )
}

pub(crate) fn update_provider_rule_set(
    id: i64,
    input: &ProviderRuleSetInput,
) -> Result<Statement, StoreError> {
    update(
        "provider_rule_sets",
        id,
        &[
            "provider_id",
            "rule_set_id",
            "sort_order",
            "enabled",
            "origin",
            "updated_at",
        ],
        vec![
            value(input.provider_id),
            value(input.rule_set_id),
            value(input.sort_order),
            value(input.enabled),
            value("operator"),
            value(now()),
        ],
    )
}

pub(crate) fn delete_process(table: &'static str, id: i64) -> Result<Statement, StoreError> {
    let query = Query::delete()
        .from_table(Alias::new(table))
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .to_owned();
    Statement::query(&query)
}

fn now() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system clock after unix epoch")
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX)
}

fn operations_json(operations: &Vec<String>) -> Result<String, StoreError> {
    serde_json::to_string(operations).map_err(|error| StoreError::InvalidData {
        field: "filter operations",
        message: error.to_string(),
    })
}
