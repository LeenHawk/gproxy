use sea_orm::*;

use super::helpers::{apply_desc_cursor, unix_ms_to_offset_datetime};
use crate::query::*;
use crate::seaorm::SeaOrmStorage;
use crate::seaorm::entities::*;

fn apply_upstream_filters(
    mut select: Select<upstream_requests::Entity>,
    query: &UpstreamRequestQuery,
) -> Select<upstream_requests::Entity> {
    if let Scope::Eq(v) = &query.trace_id {
        select = select.filter(upstream_requests::Column::TraceId.eq(*v));
    }
    if let Scope::Eq(v) = &query.provider_id {
        select = select.filter(upstream_requests::Column::ProviderId.eq(*v));
    }
    if let Scope::Eq(v) = &query.credential_id {
        select = select.filter(upstream_requests::Column::CredentialId.eq(*v));
    }
    if let Some(contains) = query.request_url_contains.as_deref() {
        select = select.filter(upstream_requests::Column::RequestUrl.contains(contains));
    }
    if let Some(from) = query.from_unix_ms {
        select = select.filter(upstream_requests::Column::At.gte(unix_ms_to_offset_datetime(from)));
    }
    if let Some(to) = query.to_unix_ms {
        select = select.filter(upstream_requests::Column::At.lte(unix_ms_to_offset_datetime(to)));
    }
    select
}

fn apply_downstream_filters(
    mut select: Select<downstream_requests::Entity>,
    query: &DownstreamRequestQuery,
) -> Select<downstream_requests::Entity> {
    if let Scope::Eq(v) = &query.trace_id {
        select = select.filter(downstream_requests::Column::TraceId.eq(*v));
    }
    if let Scope::Eq(v) = &query.user_id {
        select = select.filter(downstream_requests::Column::UserId.eq(*v));
    }
    if let Scope::Eq(v) = &query.user_key_id {
        select = select.filter(downstream_requests::Column::UserKeyId.eq(*v));
    }
    if let Some(contains) = query.request_path_contains.as_deref() {
        select = select.filter(downstream_requests::Column::RequestPath.contains(contains));
    }
    if let Some(from) = query.from_unix_ms {
        select =
            select.filter(downstream_requests::Column::At.gte(unix_ms_to_offset_datetime(from)));
    }
    if let Some(to) = query.to_unix_ms {
        select = select.filter(downstream_requests::Column::At.lte(unix_ms_to_offset_datetime(to)));
    }
    select
}

/// Request log queries — always hit the database at runtime.
/// Request logs are not cached in memory (too large).
impl SeaOrmStorage {
    pub async fn query_upstream_requests(
        &self,
        query: &UpstreamRequestQuery,
    ) -> Result<Vec<UpstreamRequestQueryRow>, DbErr> {
        let mut select = apply_upstream_filters(
            upstream_requests::Entity::find()
                .order_by_desc(upstream_requests::Column::At)
                .order_by_desc(upstream_requests::Column::TraceId),
            query,
        );
        select = apply_desc_cursor(
            select,
            query.cursor_at_unix_ms,
            query.cursor_trace_id,
            upstream_requests::Column::At,
            upstream_requests::Column::TraceId,
        );
        if let Some(offset) = query.offset {
            select = select.offset(offset);
        }
        if let Some(limit) = query.limit {
            select = select.limit(limit);
        }

        let rows = select.all(&self.db).await?;
        let include_body = query.include_body.unwrap_or(false);
        Ok(rows
            .into_iter()
            .map(|r| UpstreamRequestQueryRow {
                trace_id: r.trace_id,
                downstream_trace_id: r.downstream_trace_id,
                at: r.at,
                internal: r.internal,
                provider_id: r.provider_id,
                credential_id: r.credential_id,
                request_method: r.request_method,
                request_headers_json: r.request_headers_json,
                request_url: r.request_url,
                request_body: if include_body { r.request_body } else { None },
                response_status: r.response_status,
                response_headers_json: r.response_headers_json,
                response_body: if include_body { r.response_body } else { None },
                initial_latency_ms: r.initial_latency_ms,
                total_latency_ms: r.total_latency_ms,
                created_at: r.created_at,
            })
            .collect())
    }

    pub async fn count_upstream_requests(
        &self,
        query: &UpstreamRequestQuery,
    ) -> Result<RequestQueryCount, DbErr> {
        let select = apply_upstream_filters(upstream_requests::Entity::find(), query);
        let count = select.count(&self.db).await?;
        Ok(RequestQueryCount { count })
    }

    pub async fn query_downstream_requests(
        &self,
        query: &DownstreamRequestQuery,
    ) -> Result<Vec<DownstreamRequestQueryRow>, DbErr> {
        let mut select = apply_downstream_filters(
            downstream_requests::Entity::find()
                .order_by_desc(downstream_requests::Column::At)
                .order_by_desc(downstream_requests::Column::TraceId),
            query,
        );
        select = apply_desc_cursor(
            select,
            query.cursor_at_unix_ms,
            query.cursor_trace_id,
            downstream_requests::Column::At,
            downstream_requests::Column::TraceId,
        );
        if let Some(offset) = query.offset {
            select = select.offset(offset);
        }
        if let Some(limit) = query.limit {
            select = select.limit(limit);
        }

        let rows = select.all(&self.db).await?;
        let include_body = query.include_body.unwrap_or(false);
        Ok(rows
            .into_iter()
            .map(|r| DownstreamRequestQueryRow {
                trace_id: r.trace_id,
                at: r.at,
                internal: r.internal,
                user_id: r.user_id,
                user_key_id: r.user_key_id,
                request_method: r.request_method,
                request_headers_json: r.request_headers_json,
                request_path: r.request_path,
                request_query: r.request_query,
                request_body: if include_body { r.request_body } else { None },
                response_status: r.response_status,
                response_headers_json: r.response_headers_json,
                response_body: if include_body { r.response_body } else { None },
                created_at: r.created_at,
            })
            .collect())
    }

    pub async fn count_downstream_requests(
        &self,
        query: &DownstreamRequestQuery,
    ) -> Result<RequestQueryCount, DbErr> {
        let select = apply_downstream_filters(downstream_requests::Entity::find(), query);
        let count = select.count(&self.db).await?;
        Ok(RequestQueryCount { count })
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::ActiveValue::{NotSet, Set};
    use sea_orm::*;
    use serde_json::json;
    use time::{Duration, OffsetDateTime};

    use super::*;

    async fn test_storage() -> SeaOrmStorage {
        let storage = SeaOrmStorage::connect("sqlite::memory:", None)
            .await
            .expect("connect test storage");
        storage.sync().await.expect("sync test storage");
        storage
    }

    async fn insert_upstream_request(storage: &SeaOrmStorage, request_url: &str) -> i64 {
        upstream_requests::Entity::insert(upstream_requests::ActiveModel {
            trace_id: NotSet,
            downstream_trace_id: Set(None),
            at: Set(OffsetDateTime::UNIX_EPOCH + Duration::seconds(1)),
            internal: Set(false),
            provider_id: Set(None),
            credential_id: Set(None),
            request_method: Set("POST".to_string()),
            request_headers_json: Set(json!({})),
            request_url: Set(Some(request_url.to_string())),
            request_body: Set(None),
            response_status: Set(Some(200)),
            response_headers_json: Set(json!({})),
            response_body: Set(None),
            initial_latency_ms: Set(None),
            total_latency_ms: Set(None),
            created_at: Set(OffsetDateTime::UNIX_EPOCH + Duration::seconds(1)),
        })
        .exec(storage.connection())
        .await
        .expect("insert upstream request")
        .last_insert_id
    }

    async fn insert_downstream_request(storage: &SeaOrmStorage, request_path: &str) -> i64 {
        downstream_requests::Entity::insert(downstream_requests::ActiveModel {
            trace_id: NotSet,
            at: Set(OffsetDateTime::UNIX_EPOCH + Duration::seconds(1)),
            internal: Set(false),
            user_id: Set(None),
            user_key_id: Set(None),
            request_method: Set("GET".to_string()),
            request_headers_json: Set(json!({})),
            request_path: Set(request_path.to_string()),
            request_query: Set(None),
            request_body: Set(None),
            response_status: Set(Some(200)),
            response_headers_json: Set(json!({})),
            response_body: Set(None),
            created_at: Set(OffsetDateTime::UNIX_EPOCH + Duration::seconds(1)),
        })
        .exec(storage.connection())
        .await
        .expect("insert downstream request")
        .last_insert_id
    }

    #[tokio::test]
    async fn upstream_count_matches_query_filters() {
        let storage = test_storage().await;
        let matching_trace_id =
            insert_upstream_request(&storage, "https://example.test/v1/matches").await;
        insert_upstream_request(&storage, "https://example.test/v1/other").await;

        let query = UpstreamRequestQuery {
            trace_id: Scope::Eq(matching_trace_id),
            request_url_contains: Some("matches".to_string()),
            ..Default::default()
        };

        let rows = storage
            .query_upstream_requests(&query)
            .await
            .expect("query upstream requests");
        let count = storage
            .count_upstream_requests(&query)
            .await
            .expect("count upstream requests");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].trace_id, matching_trace_id);
        assert_eq!(count.count as usize, rows.len());
    }

    #[tokio::test]
    async fn downstream_count_matches_query_filters() {
        let storage = test_storage().await;
        let matching_trace_id = insert_downstream_request(&storage, "/v1/matches").await;
        insert_downstream_request(&storage, "/v1/other").await;

        let query = DownstreamRequestQuery {
            trace_id: Scope::Eq(matching_trace_id),
            request_path_contains: Some("matches".to_string()),
            ..Default::default()
        };

        let rows = storage
            .query_downstream_requests(&query)
            .await
            .expect("query downstream requests");
        let count = storage
            .count_downstream_requests(&query)
            .await
            .expect("count downstream requests");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].trace_id, matching_trace_id);
        assert_eq!(count.count as usize, rows.len());
    }
}
