use anyhow::Result;

use crate::providers::credential_status::ProviderKind;
use crate::providers::usage::{UsageRecord, UsageViewRecord};

use super::{UsageBackend, usage_views_for_provider};
use super::view::UsageViewCache;

pub struct MemoryUsageStore {
    cache: UsageViewCache,
}

impl MemoryUsageStore {
    pub fn new(anchor_ts: i64) -> Self {
        Self {
            cache: UsageViewCache::new(anchor_ts),
        }
    }
}

#[async_trait::async_trait]
impl UsageBackend for MemoryUsageStore {
    async fn record(&self, record: UsageRecord) -> Result<()> {
        self.cache.apply_record_all(usage_views_for_provider(record.provider), &record).await;
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
