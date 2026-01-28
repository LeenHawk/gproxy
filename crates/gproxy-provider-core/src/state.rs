use async_trait::async_trait;

use crate::disallow::DisallowRecord;

#[derive(Debug, Clone)]
pub enum ProviderStateEvent {
    UpsertDisallow(DisallowRecord),
}

#[async_trait]
pub trait StateSink: Send + Sync {
    async fn submit(&self, event: ProviderStateEvent);
}

pub struct NoopStateSink;

#[async_trait]
impl StateSink for NoopStateSink {
    async fn submit(&self, _event: ProviderStateEvent) {}
}
