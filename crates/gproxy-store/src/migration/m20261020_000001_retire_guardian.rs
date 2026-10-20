//! Remove retired operation configuration without changing historical usage.
//! An empty rewrite filter must remain match-none, never become a wildcard.
use crate::entity::{
    identity::permission,
    limits::credential_block,
    pricing::price_rule,
    upstream::{operation_endpoint, operation_rule, rewrite_rule},
};
use gproxy_seaorm::sea_orm_migration::{MigrationName, MigrationTrait, SchemaManager};
use sea_orm::{ColumnTrait, DbErr, EntityTrait, QueryFilter, sea_query::Expr};

pub struct Migration;
impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261020_000001_retire_guardian"
    }
}

const RETIRED: [&str; 2] = ["guardian_review", "guardian_classify"];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        macro_rules! remove {
            ($entity:ident) => {
                $entity::Entity::delete_many()
                    .filter($entity::Column::Operation.is_in(RETIRED))
                    .exec(db)
                    .await?;
            };
        }
        remove!(operation_endpoint);
        remove!(operation_rule);
        remove!(permission);
        remove!(credential_block);
        remove!(price_rule);
        for row in operation_rule::Entity::find()
            .filter(operation_rule::Column::Action.eq("routing"))
            .all(db)
            .await?
        {
            let Some(serde_json::Value::Object(mut mappings)) = row.target else {
                continue;
            };
            let mut changed = false;
            for mapping in mappings.values_mut() {
                if mapping
                    .pointer("/target/operation")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|operation| RETIRED.contains(&operation))
                {
                    *mapping = serde_json::json!({"implementation":"unsupported"});
                    changed = true;
                }
            }
            if changed {
                operation_rule::Entity::update_many()
                    .filter(operation_rule::Column::Id.eq(row.id))
                    .col_expr(
                        operation_rule::Column::Target,
                        Expr::value(serde_json::Value::Object(mappings)),
                    )
                    .exec(db)
                    .await?;
            }
        }
        for row in rewrite_rule::Entity::find()
            .filter(rewrite_rule::Column::FilterOperationKeys.is_not_null())
            .all(db)
            .await?
        {
            let Some(serde_json::Value::Array(mut keys)) = row.filter_operation_keys else {
                continue;
            };
            let before = keys.len();
            keys.retain(|key| {
                !key.get("operation")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|operation| RETIRED.contains(&operation))
            });
            if keys.len() == before {
                continue;
            }
            let mut update = rewrite_rule::Entity::update_many()
                .filter(rewrite_rule::Column::Id.eq(row.id))
                .col_expr(
                    rewrite_rule::Column::FilterOperationKeys,
                    Expr::value(serde_json::Value::Array(keys.clone())),
                );
            if keys.is_empty() {
                update = update.col_expr(rewrite_rule::Column::Enabled, Expr::value(false));
            }
            update.exec(db).await?;
        }
        Ok(())
    }

    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        // Deleted configuration cannot be reconstructed; usage/captures are retained.
        Ok(())
    }
}
