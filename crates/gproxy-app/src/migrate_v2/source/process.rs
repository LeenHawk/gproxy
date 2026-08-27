use gproxy_store::records::{ProviderRuleSetInput, RoutingRuleInput, RuleInput, RuleSetInput};
use tokio_rusqlite::rusqlite::{self, Connection, Result};

use super::super::model::{Legacy, SourceData};
use super::{json, optional_json};

pub(super) fn read(connection: &Connection, data: &mut SourceData) -> Result<()> {
    data.routing_rules = routing_rules(connection)?;
    data.rule_sets = rule_sets(connection)?;
    data.rules = rules(connection)?;
    data.provider_rule_sets = provider_rule_sets(connection)?;
    Ok(())
}

fn routing_rules(connection: &Connection) -> Result<Vec<Legacy<RoutingRuleInput>>> {
    let mut query = connection.prepare(
        "SELECT id,provider_id,operation,kind,implementation,dest_operation,dest_kind,sort_order,enabled FROM routing_rules ORDER BY id",
    )?;
    query
        .query_map([], |row| {
            Ok(Legacy {
                id: row.get(0)?,
                value: RoutingRuleInput {
                    provider_id: row.get(1)?,
                    operation: row.get(2)?,
                    kind: row.get(3)?,
                    implementation: row.get(4)?,
                    dest_operation: row.get(5)?,
                    dest_kind: row.get(6)?,
                    sort_order: row.get(7)?,
                    enabled: row.get(8)?,
                },
            })
        })?
        .collect()
}

fn rule_sets(connection: &Connection) -> Result<Vec<Legacy<RuleSetInput>>> {
    let mut query =
        connection.prepare("SELECT id,name,description,enabled FROM rule_sets ORDER BY id")?;
    query
        .query_map([], |row| {
            Ok(Legacy {
                id: row.get(0)?,
                value: RuleSetInput {
                    name: row.get(1)?,
                    description: row.get(2)?,
                    enabled: row.get(3)?,
                },
            })
        })?
        .collect()
}

fn rules(connection: &Connection) -> Result<Vec<Legacy<RuleInput>>> {
    let mut query = connection.prepare(
        "SELECT id,rule_set_id,kind,config_json,filter_model_pattern,filter_operation_keys,filter_header_pattern,sort_order,enabled FROM rules ORDER BY id",
    )?;
    query
        .query_map([], |row| {
            let operations = optional_json(row, 5)?
                .map(serde_json::from_value)
                .transpose()
                .map_err(|error| conversion(5, error))?;
            Ok(Legacy {
                id: row.get(0)?,
                value: RuleInput {
                    rule_set_id: row.get(1)?,
                    kind: row.get(2)?,
                    config: json(row, 3)?,
                    filter_model_pattern: row.get(4)?,
                    filter_operations: operations,
                    filter_header_pattern: row.get(6)?,
                    sort_order: row.get(7)?,
                    enabled: row.get(8)?,
                },
            })
        })?
        .collect()
}

fn provider_rule_sets(connection: &Connection) -> Result<Vec<Legacy<ProviderRuleSetInput>>> {
    let mut query = connection.prepare(
        "SELECT id,provider_id,rule_set_id,sort_order,enabled FROM provider_rule_sets ORDER BY id",
    )?;
    query
        .query_map([], |row| {
            Ok(Legacy {
                id: row.get(0)?,
                value: ProviderRuleSetInput {
                    provider_id: row.get(1)?,
                    rule_set_id: row.get(2)?,
                    sort_order: row.get(3)?,
                    enabled: row.get(4)?,
                },
            })
        })?
        .collect()
}

fn conversion(
    index: usize,
    error: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(error))
}
