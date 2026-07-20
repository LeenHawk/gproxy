use async_trait::async_trait;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use super::super::{DbPersistence, ops};
use crate::store::persistence::batch::AdminEntity;
use crate::store::persistence::db::entities::{logs, usage};
use crate::store::persistence::metrics::MetricsAggregate;
use crate::store::persistence::records::{
    AuditLog, AuditLogInput, DownstreamRequest, DownstreamRequestInput, UpstreamRequest,
    UpstreamRequestInput, Usage, UsageInput, UsageRollup, UsageRollupInput, UsageSummary,
};
use crate::store::persistence::traits::UsagePersistence;
use crate::store::persistence::{LogQuery, UsageQuery};

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
    async fn summarize_usages(&self, q: &UsageQuery) -> anyhow::Result<UsageSummary> {
        ops::usage::usages::summarize(&self.conn, q).await
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
    async fn update_upstream_response(
        &self,
        request_id: &str,
        response_body: Option<String>,
    ) -> anyhow::Result<()> {
        ops::logs::upstream_requests::update_response_body(&self.conn, request_id, response_body)
            .await
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
}
