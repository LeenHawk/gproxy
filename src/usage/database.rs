use anyhow::{Result, anyhow};
use sea_orm::{
    ActiveModelTrait, ConnectOptions, Database, DatabaseConnection, EntityTrait, Schema, Set,
    ConnectionTrait,
};
use std::time::Duration;

use crate::providers::credential_status::ProviderKind;
use crate::providers::usage::{UsageRecord, UsageViewRecord};
use crate::storage::StorageSettings;

use super::{UsageBackend, usage_views_for_provider};
use super::view::UsageViewCache;

pub struct DatabaseUsageStore {
    connection: DatabaseConnection,
    cache: UsageViewCache,
}

impl DatabaseUsageStore {
    pub async fn connect(settings: &StorageSettings, anchor_ts: i64) -> Result<Self> {
        let StorageSettings::Database {
            uri,
            max_connections,
            min_connections,
            connect_timeout_secs,
            schema,
            ssl_mode: _,
            debounce_secs: _,
        } = settings
        else {
            return Err(anyhow!("usage storage settings mismatch for database"));
        };

        let mut options = ConnectOptions::new(uri.to_string());
        if let Some(max_connections) = max_connections {
            options.max_connections(*max_connections);
        }
        if let Some(min_connections) = min_connections {
            options.min_connections(*min_connections);
        }
        if let Some(connect_timeout_secs) = connect_timeout_secs {
            options.connect_timeout(Duration::from_secs(*connect_timeout_secs));
        }
        if let Some(schema) = schema {
            options.set_schema_search_path(schema.clone());
        }

        let connection = Database::connect(options).await?;
        init_usage_schema(&connection).await?;
        let cache = UsageViewCache::new(anchor_ts);
        load_cache(&connection, &cache).await?;

        Ok(Self { connection, cache })
    }
}

#[async_trait::async_trait]
impl UsageBackend for DatabaseUsageStore {
    async fn record(&self, record: UsageRecord) -> Result<()> {
        self.cache.apply_record_all(usage_views_for_provider(record.provider), &record).await;
        let payload = serde_json::to_string(&record)?;
        let active = record_entity::ActiveModel {
            created_at: Set(record.created_at),
            provider: Set(record.provider.as_str().to_string()),
            caller_api_key: Set(record.caller_api_key.clone()),
            provider_credential_id: Set(record.provider_credential_id.clone()),
            model: Set(record.model.clone()),
            upstream_model: Set(record.upstream_model.clone()),
            format: Set(record.format.clone()),
            record_json: Set(payload),
            ..Default::default()
        };
        active.insert(&self.connection).await?;
        Ok(())
    }

    async fn flush(&self) -> Result<()> {
        Ok(())
    }

    async fn set_anchor(
        &self,
        provider: ProviderKind,
        view_name: &str,
        anchor_ts: i64,
    ) -> Result<()> {
        self.cache.set_anchor(provider, view_name, anchor_ts).await;
        Ok(())
    }

    async fn query_by_api_key(&self, api_key: &str) -> Result<Vec<UsageViewRecord>> {
        Ok(self.cache.query_by_api_key(api_key).await)
    }

    async fn query_by_provider_credential(
        &self,
        provider: ProviderKind,
        credential_id: &str,
    ) -> Result<Vec<UsageViewRecord>> {
        Ok(self
            .cache
            .query_by_provider_credential(provider, credential_id)
            .await)
    }
}

async fn init_usage_schema(connection: &DatabaseConnection) -> Result<()> {
    let backend = connection.get_database_backend();
    let schema = Schema::new(backend);
    let mut table = schema.create_table_from_entity(record_entity::Entity);
    table.if_not_exists();
    connection.execute(&table).await?;
    Ok(())
}

async fn load_cache(connection: &DatabaseConnection, cache: &UsageViewCache) -> Result<()> {
    let rows = record_entity::Entity::find().all(connection).await?;
    for row in rows {
        let record: UsageRecord = serde_json::from_str(&row.record_json)?;
        cache.apply_record_all(usage_views_for_provider(record.provider), &record).await;
    }
    Ok(())
}

pub mod record_entity {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "usage_records")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = true)]
        pub id: i64,
        pub created_at: i64,
        pub provider: String,
        pub caller_api_key: Option<String>,
        pub provider_credential_id: Option<String>,
        pub model: Option<String>,
        pub upstream_model: Option<String>,
        pub format: Option<String>,
        pub record_json: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}
