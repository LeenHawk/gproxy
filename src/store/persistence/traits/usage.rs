use crate::store::persistence::batch::AdminEntity;
use crate::store::persistence::metrics::MetricsAggregate;
use crate::store::persistence::records::{
    AuditLog, AuditLogInput, CredentialQuotaCycle, CredentialQuotaCycleInput,
    CredentialQuotaCycleModel, CredentialQuotaCycleModelInput, CredentialUsageDaily,
    CredentialUsageDailyInput, DownstreamRequest, DownstreamRequestInput, UpstreamRequest,
    UpstreamRequestInput, Usage, UsageInput, UsageModelSummary, UsageRollup, UsageRollupInput,
    UsageSummary,
};
use crate::store::persistence::{
    AuditLogQuery, CredentialQuotaCycleQuery, CredentialUsageDailyQuery, LogQuery, PageQuery,
    PageResult, UsageQuery,
};

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait UsagePersistence {
    async fn append_usage(&self, input: UsageInput) -> anyhow::Result<Option<Usage>>;
    async fn list_usages(&self, limit: u64) -> anyhow::Result<Vec<Usage>>;
    async fn query_usages(&self, q: &UsageQuery) -> anyhow::Result<Vec<Usage>>;
    async fn query_usages_page(
        &self,
        q: &UsageQuery,
        page: &PageQuery,
    ) -> anyhow::Result<PageResult<Usage>>;
    async fn summarize_usages(&self, q: &UsageQuery) -> anyhow::Result<UsageSummary>;
    async fn summarize_usages_by_model(
        &self,
        q: &UsageQuery,
    ) -> anyhow::Result<Vec<UsageModelSummary>>;
    async fn add_credential_usage_daily(
        &self,
        input: CredentialUsageDailyInput,
    ) -> anyhow::Result<CredentialUsageDaily>;
    async fn query_credential_usage_daily(
        &self,
        q: &CredentialUsageDailyQuery,
    ) -> anyhow::Result<Vec<CredentialUsageDaily>>;
    async fn get_open_credential_quota_cycle(
        &self,
        credential_id: i64,
        window_key: &str,
    ) -> anyhow::Result<Option<CredentialQuotaCycle>>;
    async fn get_credential_quota_cycle(
        &self,
        id: i64,
    ) -> anyhow::Result<Option<CredentialQuotaCycle>>;
    async fn upsert_credential_quota_cycle(
        &self,
        input: CredentialQuotaCycleInput,
    ) -> anyhow::Result<CredentialQuotaCycle>;
    async fn query_credential_quota_cycles(
        &self,
        q: &CredentialQuotaCycleQuery,
    ) -> anyhow::Result<Vec<CredentialQuotaCycle>>;
    async fn finalize_credential_quota_cycle(
        &self,
        id: i64,
        period_end: Option<i64>,
        close_reason: &str,
        finalized_at: i64,
    ) -> anyhow::Result<Option<CredentialQuotaCycle>>;
    async fn upsert_credential_quota_cycle_model(
        &self,
        input: CredentialQuotaCycleModelInput,
    ) -> anyhow::Result<CredentialQuotaCycleModel>;
    async fn list_credential_quota_cycle_models(
        &self,
        cycle_id: i64,
    ) -> anyhow::Result<Vec<CredentialQuotaCycleModel>>;
    async fn add_usage_rollup(&self, input: UsageRollupInput) -> anyhow::Result<UsageRollup>;
    async fn list_usage_rollups(
        &self,
        granularity: &str,
        from: i64,
        to: i64,
        user_id: Option<i64>,
    ) -> anyhow::Result<Vec<UsageRollup>>;
    async fn clear_usages(&self) -> anyhow::Result<()>;
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
    async fn query_downstream_requests_page(
        &self,
        q: &LogQuery,
        page: &PageQuery,
    ) -> anyhow::Result<PageResult<DownstreamRequest>>;
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
    /// Backfill one streaming capture selected by its returned row identity.
    async fn update_upstream_response_by_id(
        &self,
        capture_id: i64,
        request_id: &str,
        response_body: Option<String>,
    ) -> anyhow::Result<()>;
    async fn clear_request_logs(&self) -> anyhow::Result<()>;

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
    async fn query_audit_logs_page(
        &self,
        q: &AuditLogQuery,
        page: &PageQuery,
    ) -> anyhow::Result<PageResult<AuditLog>>;
    async fn clear_audit_logs(&self) -> anyhow::Result<()>;
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
    async fn query_usages_page(
        &self,
        q: &UsageQuery,
        page: &PageQuery,
    ) -> anyhow::Result<PageResult<Usage>> {
        super::PersistenceBackend::query_usages_page(self, q, page).await
    }
    async fn summarize_usages(&self, q: &UsageQuery) -> anyhow::Result<UsageSummary> {
        super::PersistenceBackend::summarize_usages(self, q).await
    }
    async fn summarize_usages_by_model(
        &self,
        q: &UsageQuery,
    ) -> anyhow::Result<Vec<UsageModelSummary>> {
        super::PersistenceBackend::summarize_usages_by_model(self, q).await
    }
    async fn add_credential_usage_daily(
        &self,
        input: CredentialUsageDailyInput,
    ) -> anyhow::Result<CredentialUsageDaily> {
        super::PersistenceBackend::add_credential_usage_daily(self, input).await
    }
    async fn query_credential_usage_daily(
        &self,
        q: &CredentialUsageDailyQuery,
    ) -> anyhow::Result<Vec<CredentialUsageDaily>> {
        super::PersistenceBackend::query_credential_usage_daily(self, q).await
    }
    async fn get_open_credential_quota_cycle(
        &self,
        credential_id: i64,
        window_key: &str,
    ) -> anyhow::Result<Option<CredentialQuotaCycle>> {
        super::PersistenceBackend::get_open_credential_quota_cycle(self, credential_id, window_key)
            .await
    }
    async fn get_credential_quota_cycle(
        &self,
        id: i64,
    ) -> anyhow::Result<Option<CredentialQuotaCycle>> {
        super::PersistenceBackend::get_credential_quota_cycle(self, id).await
    }
    async fn upsert_credential_quota_cycle(
        &self,
        input: CredentialQuotaCycleInput,
    ) -> anyhow::Result<CredentialQuotaCycle> {
        super::PersistenceBackend::upsert_credential_quota_cycle(self, input).await
    }
    async fn query_credential_quota_cycles(
        &self,
        q: &CredentialQuotaCycleQuery,
    ) -> anyhow::Result<Vec<CredentialQuotaCycle>> {
        super::PersistenceBackend::query_credential_quota_cycles(self, q).await
    }
    async fn finalize_credential_quota_cycle(
        &self,
        id: i64,
        period_end: Option<i64>,
        close_reason: &str,
        finalized_at: i64,
    ) -> anyhow::Result<Option<CredentialQuotaCycle>> {
        super::PersistenceBackend::finalize_credential_quota_cycle(
            self,
            id,
            period_end,
            close_reason,
            finalized_at,
        )
        .await
    }
    async fn upsert_credential_quota_cycle_model(
        &self,
        input: CredentialQuotaCycleModelInput,
    ) -> anyhow::Result<CredentialQuotaCycleModel> {
        super::PersistenceBackend::upsert_credential_quota_cycle_model(self, input).await
    }
    async fn list_credential_quota_cycle_models(
        &self,
        cycle_id: i64,
    ) -> anyhow::Result<Vec<CredentialQuotaCycleModel>> {
        super::PersistenceBackend::list_credential_quota_cycle_models(self, cycle_id).await
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
    async fn clear_usages(&self) -> anyhow::Result<()> {
        super::PersistenceBackend::clear_usages(self).await
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
    async fn query_downstream_requests_page(
        &self,
        q: &LogQuery,
        page: &PageQuery,
    ) -> anyhow::Result<PageResult<DownstreamRequest>> {
        super::PersistenceBackend::query_downstream_requests_page(self, q, page).await
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
    async fn update_upstream_response_by_id(
        &self,
        capture_id: i64,
        request_id: &str,
        response_body: Option<String>,
    ) -> anyhow::Result<()> {
        super::PersistenceBackend::update_upstream_response_by_id(
            self,
            capture_id,
            request_id,
            response_body,
        )
        .await
    }
    async fn clear_request_logs(&self) -> anyhow::Result<()> {
        super::PersistenceBackend::clear_request_logs(self).await
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
    async fn query_audit_logs_page(
        &self,
        q: &AuditLogQuery,
        page: &PageQuery,
    ) -> anyhow::Result<PageResult<AuditLog>> {
        super::PersistenceBackend::query_audit_logs_page(self, q, page).await
    }
    async fn clear_audit_logs(&self) -> anyhow::Result<()> {
        super::PersistenceBackend::clear_audit_logs(self).await
    }
}
