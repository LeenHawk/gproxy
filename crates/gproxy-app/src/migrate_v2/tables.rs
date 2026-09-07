use tokio_rusqlite::rusqlite::{Connection, Result};

use super::report::SkippedTable;

enum Policy {
    Import,
    Skip(&'static str),
}

fn policy(table: &str) -> Policy {
    match table {
        "providers"
        | "credentials"
        | "routes"
        | "route_members"
        | "aliases"
        | "price_rules"
        | "price_rule_rates"
        | "orgs"
        | "teams"
        | "users"
        | "user_keys"
        | "provider_models"
        | "quotas"
        | "route_permissions"
        | "routing_rules"
        | "rule_sets"
        | "rules"
        | "provider_rule_sets"
        | "instance_settings"
        | "usages"
        | "downstream_requests"
        | "upstream_requests" => Policy::Import,
        "credential_statuses" | "credential_model_statuses" => {
            Policy::Skip("credential health will be checked again in v3")
        }
        "usage_rollups" => Policy::Skip("v3 rebuilds aggregates from retained usage details only"),
        "credential_usage_daily" | "credential_quota_cycles" | "credential_quota_cycle_models" => {
            Policy::Skip("historical daily usage and quota cycles are not migrated")
        }
        "tokenizer_vocabs" => Policy::Skip("tokenizer caches are not migrated"),
        "codex_task_bindings" => Policy::Skip("v2 Codex task bindings are not migrated"),
        "audit_logs" => Policy::Skip("audit logs are not migrated"),
        "schema_migrations" => Policy::Skip("v3 maintains its own schema history"),
        "rate_limits" => Policy::Skip("v2 rate-limit rules are not migrated"),
        _ => Policy::Skip("unrecognized source table is not migrated"),
    }
}

pub(super) fn inspect(connection: &Connection) -> Result<Vec<SkippedTable>> {
    let mut query = connection.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let tables = query.query_map([], |row| row.get::<_, String>(0))?;
    let mut skipped = Vec::new();
    for table in tables {
        let table = table?;
        let policy = policy(&table);
        let quoted = table.replace('"', "\"\"");
        match policy {
            Policy::Import => continue,
            Policy::Skip(reason) => {
                let rows = connection.query_row(
                    &format!("SELECT COUNT(*) FROM \"{quoted}\""),
                    [],
                    |row| row.get::<_, u64>(0),
                )?;
                if rows > 0 {
                    skipped.push(SkippedTable {
                        table,
                        rows,
                        reason,
                    });
                }
            }
        }
    }
    Ok(skipped)
}
