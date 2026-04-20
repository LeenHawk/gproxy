//! Strip realtime-related routing rules from `providers.routing_json`.
//!
//! Branch `realtime` introduced six new `OperationFamily` variants
//! (`openai_realtime_websocket`, `realtime_client_secret_create`,
//! `realtime_call_accept` / `hangup` / `refer` / `reject`, and
//! `realtime_call_create`). Databases that briefly ran the realtime branch
//! have provider `routing_json` rows referencing those variants; after the
//! branch is removed from `main`, those serialized strings fail to
//! deserialize at startup with `unknown variant ...`.
//!
//! This migration rewrites every `providers.routing_json` to drop any
//! routing rule whose source OR destination operation is one of the six
//! realtime variants. Non-realtime rules are preserved. Providers with no
//! such rules are left unchanged.
//!
//! Run-once: `seaql_migrations` ledger records this. Safe on fresh DBs
//! (no rows → no rewrites).

use sea_orm::ConnectionTrait;
use sea_orm::Statement;
use sea_orm_migration::prelude::*;
use serde_json::Value;

const REALTIME_OPERATIONS: &[&str] = &[
    "openai_realtime_websocket",
    "realtime_client_secret_create",
    "realtime_call_accept",
    "realtime_call_hangup",
    "realtime_call_refer",
    "realtime_call_reject",
    "realtime_call_create",
];

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager.has_table("providers").await? {
            return Ok(());
        }

        let db = manager.get_connection();
        let backend = db.get_database_backend();

        let rows = db
            .query_all_raw(Statement::from_string(
                backend,
                "SELECT id, routing_json FROM providers",
            ))
            .await?;

        for row in rows {
            let id: i64 = row.try_get("", "id")?;
            let rj: String = row.try_get("", "routing_json")?;
            let Ok(mut doc) = serde_json::from_str::<Value>(&rj) else {
                continue;
            };
            let Some(rules) = doc.get_mut("rules").and_then(Value::as_array_mut) else {
                continue;
            };
            let before = rules.len();
            rules.retain(|rule| !rule_references_realtime(rule));
            if rules.len() == before {
                continue;
            }
            let Ok(new_rj) = serde_json::to_string(&doc) else {
                continue;
            };
            db.execute_raw(Statement::from_sql_and_values(
                backend,
                "UPDATE providers SET routing_json = ? WHERE id = ?",
                [new_rj.into(), id.into()],
            ))
            .await?;
        }

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Dropped rules are not recoverable: rollback is a no-op.
        Ok(())
    }
}

fn rule_references_realtime(rule: &Value) -> bool {
    let mentions = |op: Option<&Value>| {
        op.and_then(Value::as_str)
            .is_some_and(|value| REALTIME_OPERATIONS.contains(&value))
    };
    if mentions(rule.get("route").and_then(|route| route.get("operation"))) {
        return true;
    }
    if let Some(impl_val) = rule.get("implementation")
        && let Some(transform) = impl_val.get("TransformTo")
        && mentions(
            transform
                .get("destination")
                .and_then(|dst| dst.get("operation")),
        )
    {
        return true;
    }
    false
}
