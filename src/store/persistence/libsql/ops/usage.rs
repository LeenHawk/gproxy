use crate::store::libsql::arg_integer;
use crate::store::persistence::batch::AdminEntity;
use crate::store::persistence::metrics::MetricsAggregate;
use crate::store::persistence::records::{
    AuditLog, AuditLogInput, CredentialQuotaCycle, CredentialQuotaCycleInput,
    CredentialQuotaCycleModel, CredentialQuotaCycleModelInput, CredentialUsageDaily,
    CredentialUsageDailyInput, DownstreamRequest, DownstreamRequestInput, UpstreamRequest,
    UpstreamRequestInput, Usage, UsageInput, UsageModelSummary, UsageRollup, UsageRollupInput,
    UsageSummary,
};
use crate::store::persistence::traits::UsagePersistence;
use crate::store::persistence::{
    AuditLogQuery, CredentialQuotaCycleQuery, CredentialUsageDailyQuery, LogQuery, PageQuery,
    PageResult, UsageQuery,
};

use super::super::{LibsqlPersistence, batch, logs, metrics, usage, util};

#[async_trait::async_trait(?Send)]
impl UsagePersistence for LibsqlPersistence {
    async fn append_usage(&self, input: UsageInput) -> anyhow::Result<Option<Usage>> {
        usage::usages::append(&self.client, input).await
    }
    async fn list_usages(&self, limit: u64) -> anyhow::Result<Vec<Usage>> {
        usage::usages::list(&self.client, limit).await
    }
    async fn query_usages(&self, q: &UsageQuery) -> anyhow::Result<Vec<Usage>> {
        usage::usages::query(&self.client, q).await
    }
    async fn query_usages_page(
        &self,
        q: &UsageQuery,
        page: &PageQuery,
    ) -> anyhow::Result<PageResult<Usage>> {
        usage::usages::query_page(&self.client, q, page).await
    }
    async fn summarize_usages(&self, q: &UsageQuery) -> anyhow::Result<UsageSummary> {
        usage::usages::summarize(&self.client, q).await
    }
    async fn summarize_usages_by_model(
        &self,
        q: &UsageQuery,
    ) -> anyhow::Result<Vec<UsageModelSummary>> {
        usage::usages::summarize_by_model(&self.client, q).await
    }
    async fn add_credential_usage_daily(
        &self,
        input: CredentialUsageDailyInput,
    ) -> anyhow::Result<CredentialUsageDaily> {
        usage::credential_history::add_daily(&self.client, input).await
    }
    async fn query_credential_usage_daily(
        &self,
        q: &CredentialUsageDailyQuery,
    ) -> anyhow::Result<Vec<CredentialUsageDaily>> {
        usage::credential_history::query_daily(&self.client, q).await
    }
    async fn get_open_credential_quota_cycle(
        &self,
        credential_id: i64,
        window_key: &str,
    ) -> anyhow::Result<Option<CredentialQuotaCycle>> {
        usage::credential_history::get_open_cycle(&self.client, credential_id, window_key).await
    }
    async fn get_credential_quota_cycle(
        &self,
        id: i64,
    ) -> anyhow::Result<Option<CredentialQuotaCycle>> {
        usage::credential_history::get_cycle(&self.client, id).await
    }
    async fn upsert_credential_quota_cycle(
        &self,
        input: CredentialQuotaCycleInput,
    ) -> anyhow::Result<CredentialQuotaCycle> {
        usage::credential_history::upsert_cycle(&self.client, input).await
    }
    async fn query_credential_quota_cycles(
        &self,
        q: &CredentialQuotaCycleQuery,
    ) -> anyhow::Result<Vec<CredentialQuotaCycle>> {
        usage::credential_history::query_cycles(&self.client, q).await
    }
    async fn finalize_credential_quota_cycle(
        &self,
        id: i64,
        period_end: Option<i64>,
        close_reason: &str,
        finalized_at: i64,
    ) -> anyhow::Result<Option<CredentialQuotaCycle>> {
        usage::credential_history::finalize_cycle(
            &self.client,
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
        usage::credential_history::upsert_cycle_model(&self.client, input).await
    }
    async fn list_credential_quota_cycle_models(
        &self,
        cycle_id: i64,
    ) -> anyhow::Result<Vec<CredentialQuotaCycleModel>> {
        usage::credential_history::list_cycle_models(&self.client, cycle_id).await
    }
    async fn add_usage_rollup(&self, input: UsageRollupInput) -> anyhow::Result<UsageRollup> {
        usage::usage_rollups::add(&self.client, input).await
    }
    async fn list_usage_rollups(
        &self,
        granularity: &str,
        from: i64,
        to: i64,
        user_id: Option<i64>,
    ) -> anyhow::Result<Vec<UsageRollup>> {
        usage::usage_rollups::list(&self.client, granularity, from, to, user_id).await
    }
    async fn clear_usages(&self) -> anyhow::Result<()> {
        self.client
            .execute_batch(&["DELETE FROM usages", "DELETE FROM usage_rollups"])
            .await?;
        Ok(())
    }
    async fn metrics_aggregate(&self) -> anyhow::Result<MetricsAggregate> {
        metrics::aggregate(&self.client).await
    }

    async fn append_downstream_request(
        &self,
        input: DownstreamRequestInput,
    ) -> anyhow::Result<DownstreamRequest> {
        logs::downstream_requests::append(&self.client, input).await
    }
    async fn list_downstream_requests(
        &self,
        request_id: &str,
    ) -> anyhow::Result<Vec<DownstreamRequest>> {
        logs::downstream_requests::list(&self.client, request_id).await
    }
    async fn query_downstream_requests(
        &self,
        q: &LogQuery,
    ) -> anyhow::Result<Vec<DownstreamRequest>> {
        logs::downstream_requests::query(&self.client, q).await
    }
    async fn query_downstream_requests_page(
        &self,
        q: &LogQuery,
        page: &PageQuery,
    ) -> anyhow::Result<PageResult<DownstreamRequest>> {
        logs::downstream_requests::query_page(&self.client, q, page).await
    }
    async fn update_downstream_response(
        &self,
        request_id: &str,
        response_body: Option<String>,
    ) -> anyhow::Result<()> {
        logs::downstream_requests::update_response_body(&self.client, request_id, response_body)
            .await
    }
    async fn append_upstream_request(
        &self,
        input: UpstreamRequestInput,
    ) -> anyhow::Result<UpstreamRequest> {
        logs::upstream_requests::append(&self.client, input).await
    }
    async fn list_upstream_requests(
        &self,
        request_id: &str,
    ) -> anyhow::Result<Vec<UpstreamRequest>> {
        logs::upstream_requests::list(&self.client, request_id).await
    }
    async fn update_upstream_response_by_id(
        &self,
        capture_id: i64,
        request_id: &str,
        response_body: Option<String>,
    ) -> anyhow::Result<()> {
        logs::upstream_requests::update_response_body(
            &self.client,
            capture_id,
            request_id,
            response_body,
        )
        .await
    }
    async fn clear_request_logs(&self) -> anyhow::Result<()> {
        self.client
            .execute_batch(&[
                "DELETE FROM upstream_requests",
                "DELETE FROM downstream_requests",
            ])
            .await?;
        Ok(())
    }

    async fn delete_usage(&self, id: i64) -> anyhow::Result<bool> {
        batch::delete_usage(&self.client, id).await
    }
    async fn set_enabled(
        &self,
        entity: AdminEntity,
        id: i64,
        enabled: bool,
    ) -> anyhow::Result<bool> {
        batch::set_enabled(&self.client, entity, id, enabled).await
    }
    async fn purge_before(&self, cutoff_ts: i64) -> anyhow::Result<u64> {
        let mut removed = 0u64;
        for table in ["usages", "downstream_requests", "upstream_requests"] {
            removed += util::exec(
                &self.client,
                &format!("DELETE FROM {table} WHERE created_at < ?"),
                &[arg_integer(cutoff_ts)],
            )
            .await?;
        }
        Ok(removed)
    }
    async fn append_audit_log(&self, input: AuditLogInput) -> anyhow::Result<AuditLog> {
        logs::audit_logs::append(&self.client, input).await
    }
    async fn list_audit_logs(&self, limit: u64) -> anyhow::Result<Vec<AuditLog>> {
        logs::audit_logs::list(&self.client, limit).await
    }
    async fn query_audit_logs_page(
        &self,
        q: &AuditLogQuery,
        page: &PageQuery,
    ) -> anyhow::Result<PageResult<AuditLog>> {
        logs::audit_logs::query_page(&self.client, q, page).await
    }
    async fn clear_audit_logs(&self) -> anyhow::Result<()> {
        util::exec(&self.client, "DELETE FROM audit_logs", &[]).await?;
        Ok(())
    }
}
