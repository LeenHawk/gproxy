use async_trait::async_trait;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, TransactionTrait};

use super::super::{DbPersistence, ops};
use crate::store::persistence::batch::AdminEntity;
use crate::store::persistence::db::entities::{logs, usage};
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

#[async_trait]
impl UsagePersistence for DbPersistence {
    async fn append_usage(&self, input: UsageInput) -> anyhow::Result<Option<Usage>> {
        ops::usage::usages::append(&self.conn, input).await
    }
    async fn list_usages(&self, limit: u64) -> anyhow::Result<Vec<Usage>> {
        ops::usage::usages::list(&self.conn, limit).await
    }
    async fn query_usages(&self, q: &UsageQuery) -> anyhow::Result<Vec<Usage>> {
        ops::usage::usages::query(&self.conn, q).await
    }
    async fn query_usages_page(
        &self,
        q: &UsageQuery,
        page: &PageQuery,
    ) -> anyhow::Result<PageResult<Usage>> {
        ops::usage::usages::query_page(&self.conn, q, page).await
    }
    async fn summarize_usages(&self, q: &UsageQuery) -> anyhow::Result<UsageSummary> {
        ops::usage::usages::summarize(&self.conn, q).await
    }
    async fn summarize_usages_by_model(
        &self,
        q: &UsageQuery,
    ) -> anyhow::Result<Vec<UsageModelSummary>> {
        ops::usage::usages::summarize_by_model(&self.conn, q).await
    }
    async fn add_credential_usage_daily(
        &self,
        input: CredentialUsageDailyInput,
    ) -> anyhow::Result<CredentialUsageDaily> {
        ops::usage::credential_history::add_daily(&self.conn, input).await
    }
    async fn query_credential_usage_daily(
        &self,
        q: &CredentialUsageDailyQuery,
    ) -> anyhow::Result<Vec<CredentialUsageDaily>> {
        ops::usage::credential_history::query_daily(&self.conn, q).await
    }
    async fn get_open_credential_quota_cycle(
        &self,
        credential_id: i64,
        window_key: &str,
    ) -> anyhow::Result<Option<CredentialQuotaCycle>> {
        ops::usage::credential_history::get_open_cycle(&self.conn, credential_id, window_key).await
    }
    async fn get_credential_quota_cycle(
        &self,
        id: i64,
    ) -> anyhow::Result<Option<CredentialQuotaCycle>> {
        ops::usage::credential_history::get_cycle(&self.conn, id).await
    }
    async fn upsert_credential_quota_cycle(
        &self,
        input: CredentialQuotaCycleInput,
    ) -> anyhow::Result<CredentialQuotaCycle> {
        ops::usage::credential_history::upsert_cycle(&self.conn, input).await
    }
    async fn query_credential_quota_cycles(
        &self,
        q: &CredentialQuotaCycleQuery,
    ) -> anyhow::Result<Vec<CredentialQuotaCycle>> {
        ops::usage::credential_history::query_cycles(&self.conn, q).await
    }
    async fn finalize_credential_quota_cycle(
        &self,
        id: i64,
        period_end: Option<i64>,
        close_reason: &str,
        finalized_at: i64,
    ) -> anyhow::Result<Option<CredentialQuotaCycle>> {
        ops::usage::credential_history::finalize_cycle(
            &self.conn,
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
        ops::usage::credential_history::upsert_cycle_model(&self.conn, input).await
    }
    async fn list_credential_quota_cycle_models(
        &self,
        cycle_id: i64,
    ) -> anyhow::Result<Vec<CredentialQuotaCycleModel>> {
        ops::usage::credential_history::list_cycle_models(&self.conn, cycle_id).await
    }
    async fn add_usage_rollup(&self, input: UsageRollupInput) -> anyhow::Result<UsageRollup> {
        ops::usage::usage_rollups::add(&self.conn, input).await
    }
    async fn list_usage_rollups(
        &self,
        granularity: &str,
        from: i64,
        to: i64,
        user_id: Option<i64>,
    ) -> anyhow::Result<Vec<UsageRollup>> {
        ops::usage::usage_rollups::list(&self.conn, granularity, from, to, user_id).await
    }
    async fn clear_usages(&self) -> anyhow::Result<()> {
        let txn = self.conn.begin().await?;
        usage::usage::Entity::delete_many().exec(&txn).await?;
        usage::usage_rollup::Entity::delete_many()
            .exec(&txn)
            .await?;
        txn.commit().await?;
        Ok(())
    }
    async fn metrics_aggregate(&self) -> anyhow::Result<MetricsAggregate> {
        ops::metrics::aggregate(&self.conn).await
    }

    async fn append_downstream_request(
        &self,
        input: DownstreamRequestInput,
    ) -> anyhow::Result<DownstreamRequest> {
        ops::logs::downstream_requests::append(&self.conn, input).await
    }
    async fn list_downstream_requests(
        &self,
        request_id: &str,
    ) -> anyhow::Result<Vec<DownstreamRequest>> {
        ops::logs::downstream_requests::list(&self.conn, request_id).await
    }
    async fn query_downstream_requests(
        &self,
        q: &LogQuery,
    ) -> anyhow::Result<Vec<DownstreamRequest>> {
        ops::logs::downstream_requests::query(&self.conn, q).await
    }
    async fn query_downstream_requests_page(
        &self,
        q: &LogQuery,
        page: &PageQuery,
    ) -> anyhow::Result<PageResult<DownstreamRequest>> {
        ops::logs::downstream_requests::query_page(&self.conn, q, page).await
    }
    async fn update_downstream_response(
        &self,
        request_id: &str,
        response_body: Option<String>,
    ) -> anyhow::Result<()> {
        ops::logs::downstream_requests::update_response_body(&self.conn, request_id, response_body)
            .await
    }
    async fn append_upstream_request(
        &self,
        input: UpstreamRequestInput,
    ) -> anyhow::Result<UpstreamRequest> {
        ops::logs::upstream_requests::append(&self.conn, input).await
    }
    async fn list_upstream_requests(
        &self,
        request_id: &str,
    ) -> anyhow::Result<Vec<UpstreamRequest>> {
        ops::logs::upstream_requests::list(&self.conn, request_id).await
    }
    async fn update_upstream_response_by_id(
        &self,
        capture_id: i64,
        request_id: &str,
        response_body: Option<String>,
    ) -> anyhow::Result<()> {
        ops::logs::upstream_requests::update_response_body(
            &self.conn,
            capture_id,
            request_id,
            response_body,
        )
        .await
    }
    async fn clear_request_logs(&self) -> anyhow::Result<()> {
        let txn = self.conn.begin().await?;
        logs::upstream_request::Entity::delete_many()
            .exec(&txn)
            .await?;
        logs::downstream_request::Entity::delete_many()
            .exec(&txn)
            .await?;
        txn.commit().await?;
        Ok(())
    }

    async fn delete_usage(&self, id: i64) -> anyhow::Result<bool> {
        ops::batch::delete_usage(&self.conn, id).await
    }
    async fn set_enabled(
        &self,
        entity: AdminEntity,
        id: i64,
        enabled: bool,
    ) -> anyhow::Result<bool> {
        ops::batch::set_enabled(&self.conn, entity, id, enabled).await
    }
    async fn purge_before(&self, cutoff_ts: i64) -> anyhow::Result<u64> {
        let mut removed = 0u64;
        removed += usage::usage::Entity::delete_many()
            .filter(usage::usage::Column::CreatedAt.lt(cutoff_ts))
            .exec(&self.conn)
            .await?
            .rows_affected;
        removed += logs::downstream_request::Entity::delete_many()
            .filter(logs::downstream_request::Column::CreatedAt.lt(cutoff_ts))
            .exec(&self.conn)
            .await?
            .rows_affected;
        removed += logs::upstream_request::Entity::delete_many()
            .filter(logs::upstream_request::Column::CreatedAt.lt(cutoff_ts))
            .exec(&self.conn)
            .await?
            .rows_affected;
        Ok(removed)
    }
    async fn append_audit_log(&self, input: AuditLogInput) -> anyhow::Result<AuditLog> {
        ops::logs::audit_logs::append(&self.conn, input).await
    }
    async fn list_audit_logs(&self, limit: u64) -> anyhow::Result<Vec<AuditLog>> {
        ops::logs::audit_logs::list(&self.conn, limit).await
    }
    async fn query_audit_logs_page(
        &self,
        q: &AuditLogQuery,
        page: &PageQuery,
    ) -> anyhow::Result<PageResult<AuditLog>> {
        ops::logs::audit_logs::query_page(&self.conn, q, page).await
    }
    async fn clear_audit_logs(&self) -> anyhow::Result<()> {
        logs::audit_log::Entity::delete_many()
            .exec(&self.conn)
            .await?;
        Ok(())
    }
}
