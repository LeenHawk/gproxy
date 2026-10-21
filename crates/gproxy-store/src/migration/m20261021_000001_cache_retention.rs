//! Dynamic cache-write retention. Historical usage and settled costs are untouched.
use crate::entity::{
    pricing::{price_rate, price_tier},
    usage::usage_record,
};
use gproxy_seaorm::{
    SchemaProbeExt,
    sea_orm_migration::{MigrationName, MigrationTrait, SchemaManager},
};
use sea_orm::{
    ColumnTrait, DbErr, EntityTrait, QueryFilter, Schema,
    sea_query::{Expr, Table},
};
use serde_json::json;

pub struct Migration;
impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261021_000001_cache_retention"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let schema = Schema::new(manager.get_database_backend());
        for (table, name, definition) in [
            (
                "usage_records",
                "cache_creation",
                schema.create_table_from_entity(usage_record::Entity),
            ),
            (
                "price_tiers",
                "cache_creation_per_million",
                schema.create_table_from_entity(price_tier::Entity),
            ),
        ] {
            if !manager.probe_column(table, name).await? {
                let column = definition
                    .get_columns()
                    .iter()
                    .find(|column| column.get_column_name() == name)
                    .ok_or_else(|| {
                        DbErr::Migration(format!("{table}.{name} missing from entity"))
                    })?;
                manager
                    .alter_table(
                        Table::alter()
                            .table(table)
                            .add_column(column.clone())
                            .to_owned(),
                    )
                    .await?;
            }
        }
        let db = manager.get_connection();
        for row in price_tier::Entity::find()
            .filter(price_tier::Column::CacheCreationPerMillion.is_null())
            .all(db)
            .await?
        {
            let entries: Vec<_> = [
                (Some(300u64), row.cache_creation_5m_per_million),
                // The former 30m bucket represented OpenAI writes with no reported TTL.
                (None, row.cache_creation_30m_per_million),
                (Some(3600), row.cache_creation_1h_per_million),
            ]
            .into_iter()
            .filter_map(|(ttl_seconds, price)| {
                Some(json!({"ttl_seconds": ttl_seconds, "price": price?.to_string()}))
            })
            .collect();
            if !entries.is_empty() {
                price_tier::Entity::update_many()
                    .filter(price_tier::Column::Id.eq(row.id))
                    .col_expr(
                        price_tier::Column::CacheCreationPerMillion,
                        Expr::value(json!(entries)),
                    )
                    .exec(db)
                    .await?;
            }
        }
        for row in price_rate::Entity::find()
            .filter(price_rate::Column::Metric.is_in([
                "cache_creation_5m_tokens",
                "cache_creation_30m_tokens",
                "cache_creation_1h_tokens",
            ]))
            .all(db)
            .await?
        {
            let ttl = match row.metric.as_str() {
                "cache_creation_5m_tokens" => Some(300),
                "cache_creation_1h_tokens" => Some(3600),
                _ => None,
            };
            let mut conditions = row
                .conditions
                .clone()
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            if let Some(ttl) = ttl {
                conditions.insert("ttl_seconds".into(), json!(ttl));
            }
            price_rate::Entity::update_many()
                .filter(price_rate::Column::Id.eq(row.id))
                .col_expr(
                    price_rate::Column::Metric,
                    Expr::value("cache_creation_tokens"),
                )
                .col_expr(
                    price_rate::Column::Conditions,
                    Expr::value(
                        (!conditions.is_empty()).then_some(serde_json::Value::Object(conditions)),
                    ),
                )
                .exec(db)
                .await?;
        }
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
