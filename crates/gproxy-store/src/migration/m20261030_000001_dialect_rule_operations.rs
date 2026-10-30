//! Dialect rules no longer carry a built-in operation scope; spell out the
//! conversation scope on unfiltered system_text/cache_breakpoint rules so they
//! keep away from list, embedding, and other bodies of the same dialect.
use crate::entity::upstream::rewrite_rule;
use gproxy_seaorm::sea_orm_migration::{MigrationName, MigrationTrait, SchemaManager};
use sea_orm::{ColumnTrait, DbErr, EntityTrait, QueryFilter, sea_query::Expr};
use serde_json::{Value, json};

pub struct Migration;
impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261030_000001_dialect_rule_operations"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        for row in rewrite_rule::Entity::find()
            .filter(rewrite_rule::Column::Action.is_in(["system_text", "cache_breakpoint"]))
            .filter(rewrite_rule::Column::FilterOperationKeys.is_null())
            .all(db)
            .await?
        {
            let Some(dialect) = serde_json::from_str::<Value>(&row.replacement)
                .ok()
                .and_then(|config| config.get("dialect")?.as_str().map(str::to_owned))
            else {
                continue;
            };
            // Mirrors the console preset: count_tokens exists only for these two.
            let count = matches!(dialect.as_str(), "openai" | "claude");
            let keys: Vec<Value> = ["generate_content", "stream_generate_content"]
                .into_iter()
                .chain(count.then_some("count_tokens"))
                .map(|operation| json!({"operation": operation, "dialect": dialect}))
                .collect();
            rewrite_rule::Entity::update_many()
                .filter(rewrite_rule::Column::Id.eq(row.id))
                .col_expr(
                    rewrite_rule::Column::FilterOperationKeys,
                    Expr::value(Value::Array(keys)),
                )
                .exec(db)
                .await?;
        }
        Ok(())
    }

    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        // An explicit filter is valid configuration under either schema.
        Ok(())
    }
}
