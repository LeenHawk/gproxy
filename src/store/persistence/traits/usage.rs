use crate::store::persistence::batch::AdminEntity;
use crate::store::persistence::metrics::MetricsAggregate;
use crate::store::persistence::records::{
    AuditLog, AuditLogInput, DownstreamRequest, DownstreamRequestInput, UpstreamRequest,
    UpstreamRequestInput, Usage, UsageInput, UsageRollup, UsageRollupInput, UsageSummary,
};
use crate::store::persistence::{LogQuery, UsageQuery};

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait UsagePersistence {
    async fn append_usage(&self, input: UsageInput) -> anyhow::Result<Option<Usage>>;
    async fn list_usages(&self, limit: u64) -> anyhow::Result<Vec<Usage>>;
    async fn query_usages(&self, q: &UsageQuery) -> anyhow::Result<Vec<Usage>>;
    async fn summarize_usages(&self, q: &UsageQuery) -> anyhow::Result<UsageSummary>;
    async fn add_usage_rollup(&self, input: UsageRollupInput) -> anyhow::Result<UsageRollup>;
    async fn list_usage_rollups(
        &self,
        granularity: &str,
        from: i64,
        to: i64,
        user_id: Option<i64>,
    ) -> anyhow::Result<Vec<UsageRollup>>;
    async fn metrics_aggregate(&self) -> anyhow::Result<MetricsAggregate>;

    async fn append_downstream_request(
        &self,
        input: DownstreamRequestInput,
    ) -> anyhow::Result<DownstreamRequest>;
    async fn list_downstream_requests(
        &self,
        request_id: &str,
    ) -> anyhow::Result<Vec<DownstreamRequest>>;
    async fn query_downstream_requests(
        &self,
        q: &LogQuery,
    ) -> anyhow::Result<Vec<DownstreamRequest>>;
    async fn update_downstream_response(
        &self,
        request_id: &str,
        response_body: Option<String>,
    ) -> anyhow::Result<()>;
    async fn append_upstream_request(
        &self,
        input: UpstreamRequestInput,
    ) -> anyhow::Result<UpstreamRequest>;
    async fn list_upstream_requests(
        &self,
        request_id: &str,
    ) -> anyhow::Result<Vec<UpstreamRequest>>;
    async fn update_upstream_response(
        &self,
        request_id: &str,
        response_body: Option<String>,
    ) -> anyhow::Result<()>;

    async fn delete_usage(&self, id: i64) -> anyhow::Result<bool>;
    async fn set_enabled(
        &self,
        entity: AdminEntity,
        id: i64,
        enabled: bool,
    ) -> anyhow::Result<bool>;
    async fn purge_before(&self, cutoff_ts: i64) -> anyhow::Result<u64>;
    async fn append_audit_log(&self, input: AuditLogInput) -> anyhow::Result<AuditLog>;
    async fn list_audit_logs(&self, limit: u64) -> anyhow::Result<Vec<AuditLog>>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl UsagePersistence for dyn super::PersistenceBackend + '_ {
    async fn append_usage(&self, input: UsageInput) -> anyhow::Result<Option<Usage>> {
        super::PersistenceBackend::append_usage(self, input).await
    }
    async fn list_usages(&self, limit: u64) -> anyhow::Result<Vec<Usage>> {
        super::PersistenceBackend::list_usages(self, limit).await
    }
    async fn query_usages(&self, q: &UsageQuery) -> anyhow::Result<Vec<Usage>> {
        super::PersistenceBackend::query_usages(self, q).await
    }
    async fn summarize_usages(&self, q: &UsageQuery) -> anyhow::Result<UsageSummary> {
        super::PersistenceBackend::summarize_usages(self, q).await
    }
    async fn add_usage_rollup(&self, input: UsageRollupInput) -> anyhow::Result<UsageRollup> {
        super::PersistenceBackend::add_usage_rollup(self, input).await
    }
    async fn list_usage_rollups(
        &self,
        granularity: &str,
        from: i64,
        to: i64,
        user_id: Option<i64>,
    ) -> anyhow::Result<Vec<UsageRollup>> {
        super::PersistenceBackend::list_usage_rollups(self, granularity, from, to, user_id).await
    }
    async fn metrics_aggregate(&self) -> anyhow::Result<MetricsAggregate> {
        super::PersistenceBackend::metrics_aggregate(self).await
    }
    async fn append_downstream_request(
        &self,
        input: DownstreamRequestInput,
    ) -> anyhow::Result<DownstreamRequest> {
        super::PersistenceBackend::append_downstream_request(self, input).await
    }
    async fn list_downstream_requests(
        &self,
        request_id: &str,
    ) -> anyhow::Result<Vec<DownstreamRequest>> {
        super::PersistenceBackend::list_downstream_requests(self, request_id).await
    }
    async fn query_downstream_requests(
        &self,
        q: &LogQuery,
    ) -> anyhow::Result<Vec<DownstreamRequest>> {
        super::PersistenceBackend::query_downstream_requests(self, q).await
    }
    async fn update_downstream_response(
        &self,
        request_id: &str,
        response_body: Option<String>,
    ) -> anyhow::Result<()> {
        super::PersistenceBackend::update_downstream_response(self, request_id, response_body).await
    }
    async fn append_upstream_request(
        &self,
        input: UpstreamRequestInput,
    ) -> anyhow::Result<UpstreamRequest> {
        super::PersistenceBackend::append_upstream_request(self, input).await
    }
    async fn list_upstream_requests(
        &self,
        request_id: &str,
    ) -> anyhow::Result<Vec<UpstreamRequest>> {
        super::PersistenceBackend::list_upstream_requests(self, request_id).await
    }
    async fn update_upstream_response(
        &self,
        request_id: &str,
        response_body: Option<String>,
    ) -> anyhow::Result<()> {
        super::PersistenceBackend::update_upstream_response(self, request_id, response_body).await
    }
    async fn delete_usage(&self, id: i64) -> anyhow::Result<bool> {
        super::PersistenceBackend::delete_usage(self, id).await
    }
    async fn set_enabled(
        &self,
        entity: AdminEntity,
        id: i64,
        enabled: bool,
    ) -> anyhow::Result<bool> {
        super::PersistenceBackend::set_enabled(self, entity, id, enabled).await
    }
    async fn purge_before(&self, cutoff_ts: i64) -> anyhow::Result<u64> {
        super::PersistenceBackend::purge_before(self, cutoff_ts).await
    }
    async fn append_audit_log(&self, input: AuditLogInput) -> anyhow::Result<AuditLog> {
        super::PersistenceBackend::append_audit_log(self, input).await
    }
    async fn list_audit_logs(&self, limit: u64) -> anyhow::Result<Vec<AuditLog>> {
        super::PersistenceBackend::list_audit_logs(self, limit).await
    }
}
