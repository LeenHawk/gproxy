use std::sync::Arc;

use gproxy_provider_core::{DownstreamTrafficEvent, TrafficSink, UpstreamTrafficEvent};
use gproxy_storage::StorageBus;

#[derive(Clone)]
pub struct StorageTrafficSink {
    downstream: Arc<tokio::sync::mpsc::Sender<DownstreamTrafficEvent>>,
    upstream: Arc<tokio::sync::mpsc::Sender<UpstreamTrafficEvent>>,
}

impl StorageTrafficSink {
    pub fn new(bus: &StorageBus) -> Self {
        Self {
            downstream: Arc::new(bus.downstream_tx.clone()),
            upstream: Arc::new(bus.upstream_tx.clone()),
        }
    }
}

impl TrafficSink for StorageTrafficSink {
    fn record_downstream(&self, event: DownstreamTrafficEvent) {
        let _ = self.downstream.try_send(event);
    }

    fn record_upstream(&self, event: UpstreamTrafficEvent) {
        let _ = self.upstream.try_send(event);
    }
}
