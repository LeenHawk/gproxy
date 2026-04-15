use sea_orm::*;
use std::collections::BTreeMap;

use super::helpers::unix_ms_to_offset_datetime;
use crate::query::*;
use crate::seaorm::SeaOrmStorage;

impl SeaOrmStorage {
    pub async fn query_dashboard_overview(
        &self,
        query: &DashboardQuery,
    ) -> Result<DashboardOverview, DbErr> {
        let from = unix_ms_to_offset_datetime(query.from_unix_ms);
        let to = unix_ms_to_offset_datetime(query.to_unix_ms);
        let bucket = query.bucket_seconds;
        let backend = self.db.get_database_backend();

        // --- 1. downstream_requests: KPI + traffic + status_codes ---
        let bucket_expr = bucket_expression(backend, "downstream_requests", bucket);
        let downstream_sql = format!(
            "SELECT \
                {bucket_expr} AS bucket, \
                COUNT(*) AS total, \
                SUM(CASE WHEN response_status >= 200 AND response_status < 300 THEN 1 ELSE 0 END) AS ok, \
                SUM(CASE WHEN response_status >= 400 AND response_status < 500 THEN 1 ELSE 0 END) AS err_4xx, \
                SUM(CASE WHEN response_status >= 500 THEN 1 ELSE 0 END) AS err_5xx \
            FROM downstream_requests \
            WHERE at >= {p1} AND at <= {p2} \
            GROUP BY bucket \
            ORDER BY bucket",
            p1 = param(backend, 1),
            p2 = param(backend, 2),
        );
        let downstream_stmt =
            Statement::from_sql_and_values(backend, &downstream_sql, [from.into(), to.into()]);
        let downstream_rows = self.db.query_all_raw(downstream_stmt).await?;

        let mut kpi = DashboardKpi::default();
        let mut traffic_by_bucket = BTreeMap::<i64, DashboardTrafficBucket>::new();
        let mut status_codes: Vec<DashboardStatusBucket> = Vec::new();
        for row in &downstream_rows {
            let b: i64 = row.try_get("", "bucket").unwrap_or(0);
            let total: i64 = row.try_get("", "total").unwrap_or(0);
            let ok: i64 = row.try_get("", "ok").unwrap_or(0);
            let err_4xx: i64 = row.try_get("", "err_4xx").unwrap_or(0);
            let err_5xx: i64 = row.try_get("", "err_5xx").unwrap_or(0);
            kpi.total_requests += total;
            kpi.success_count += ok;
            kpi.error_4xx_count += err_4xx;
            kpi.error_5xx_count += err_5xx;
            traffic_by_bucket.insert(
                b,
                DashboardTrafficBucket {
                    bucket: b,
                    request_count: total,
                    cost: 0.0,
                },
            );
            status_codes.push(DashboardStatusBucket {
                bucket: b,
                ok,
                err_4xx,
                err_5xx,
            });
        }

        // --- 2. usages: cost + tokens per bucket ---
        let usage_bucket_expr = bucket_expression(backend, "usages", bucket);
        let usages_sql = format!(
            "SELECT \
                {usage_bucket_expr} AS bucket, \
                COALESCE(SUM(cost), 0) AS cost, \
                COALESCE(SUM(input_tokens), 0) AS input_tokens, \
                COALESCE(SUM(output_tokens), 0) AS output_tokens \
            FROM usages \
            WHERE at >= {p1} AND at <= {p2} \
            GROUP BY bucket \
            ORDER BY bucket",
            p1 = param(backend, 1),
            p2 = param(backend, 2),
        );
        let usages_stmt =
            Statement::from_sql_and_values(backend, &usages_sql, [from.into(), to.into()]);
        let usage_rows = self.db.query_all_raw(usages_stmt).await?;

        for row in &usage_rows {
            let b: i64 = row.try_get("", "bucket").unwrap_or(0);
            let cost: f64 = row.try_get("", "cost").unwrap_or(0.0);
            let input: i64 = row.try_get("", "input_tokens").unwrap_or(0);
            let output: i64 = row.try_get("", "output_tokens").unwrap_or(0);
            kpi.total_cost += cost;
            kpi.total_input_tokens += input;
            kpi.total_output_tokens += output;
            traffic_by_bucket
                .entry(b)
                .and_modify(|bucket| bucket.cost = cost)
                .or_insert(DashboardTrafficBucket {
                    bucket: b,
                    request_count: 0,
                    cost,
                });
        }

        // --- 3. upstream_requests: avg + max latency (whole window) ---
        let latency_sql = format!(
            "SELECT AVG(total_latency_ms) AS avg_lat, MAX(total_latency_ms) AS max_lat \
            FROM upstream_requests \
            WHERE at >= {p1} AND at <= {p2} \
                AND total_latency_ms IS NOT NULL AND total_latency_ms > 0",
            p1 = param(backend, 1),
            p2 = param(backend, 2),
        );
        let latency_stmt =
            Statement::from_sql_and_values(backend, &latency_sql, [from.into(), to.into()]);
        if let Some(row) = self.db.query_one_raw(latency_stmt).await? {
            kpi.avg_latency_ms = row.try_get::<Option<f64>>("", "avg_lat").unwrap_or(None);
            kpi.max_latency_ms = row.try_get::<Option<i64>>("", "max_lat").unwrap_or(None);
        }

        Ok(DashboardOverview {
            kpi,
            traffic: traffic_by_bucket.into_values().collect(),
            status_codes,
        })
    }

    pub async fn query_dashboard_top_providers(
        &self,
        query: &DashboardQuery,
    ) -> Result<DashboardTopProviders, DbErr> {
        let from = unix_ms_to_offset_datetime(query.from_unix_ms);
        let to = unix_ms_to_offset_datetime(query.to_unix_ms);
        let backend = self.db.get_database_backend();

        let sql = format!(
            "SELECT \
                u.provider_id, p.channel, \
                COUNT(*) AS request_count, \
                COALESCE(SUM(u.cost), 0) AS total_cost, \
                COALESCE(SUM(u.input_tokens), 0) AS total_input_tokens, \
                COALESCE(SUM(u.output_tokens), 0) AS total_output_tokens \
            FROM usages u \
            LEFT JOIN providers p ON u.provider_id = p.id \
            WHERE u.at >= {p1} AND u.at <= {p2} \
            GROUP BY u.provider_id, p.channel \
            ORDER BY request_count DESC \
            LIMIT 10",
            p1 = param(backend, 1),
            p2 = param(backend, 2),
        );
        let stmt = Statement::from_sql_and_values(backend, &sql, [from.into(), to.into()]);
        let rows = self.db.query_all_raw(stmt).await?;

        let result = rows
            .iter()
            .map(|r| DashboardTopProviderRow {
                provider_id: r.try_get("", "provider_id").unwrap_or(None),
                channel: r.try_get("", "channel").unwrap_or(None),
                request_count: r.try_get("", "request_count").unwrap_or(0),
                total_cost: r.try_get("", "total_cost").unwrap_or(0.0),
                total_input_tokens: r.try_get("", "total_input_tokens").unwrap_or(0),
                total_output_tokens: r.try_get("", "total_output_tokens").unwrap_or(0),
            })
            .collect();

        Ok(DashboardTopProviders { rows: result })
    }

    pub async fn query_dashboard_top_models(
        &self,
        query: &DashboardQuery,
    ) -> Result<DashboardTopModels, DbErr> {
        let from = unix_ms_to_offset_datetime(query.from_unix_ms);
        let to = unix_ms_to_offset_datetime(query.to_unix_ms);
        let backend = self.db.get_database_backend();

        let sql = format!(
            "SELECT \
                model, \
                COUNT(*) AS request_count, \
                COALESCE(SUM(cost), 0) AS total_cost, \
                COALESCE(SUM(input_tokens), 0) AS total_input_tokens, \
                COALESCE(SUM(output_tokens), 0) AS total_output_tokens \
            FROM usages \
            WHERE at >= {p1} AND at <= {p2} \
            GROUP BY model \
            ORDER BY request_count DESC \
            LIMIT 10",
            p1 = param(backend, 1),
            p2 = param(backend, 2),
        );
        let stmt = Statement::from_sql_and_values(backend, &sql, [from.into(), to.into()]);
        let rows = self.db.query_all_raw(stmt).await?;

        let result = rows
            .iter()
            .map(|r| DashboardTopModelRow {
                model: r.try_get("", "model").unwrap_or(None),
                request_count: r.try_get("", "request_count").unwrap_or(0),
                total_cost: r.try_get("", "total_cost").unwrap_or(0.0),
                total_input_tokens: r.try_get("", "total_input_tokens").unwrap_or(0),
                total_output_tokens: r.try_get("", "total_output_tokens").unwrap_or(0),
            })
            .collect();

        Ok(DashboardTopModels { rows: result })
    }
}

/// Generate a SQL expression that converts the `at` datetime column to
/// a bucket key (unix epoch seconds rounded down to `bucket_seconds`).
fn bucket_expression(backend: DatabaseBackend, table: &str, bucket_seconds: i64) -> String {
    match backend {
        DatabaseBackend::Sqlite => {
            format!(
                "(CAST(strftime('%s', {table}.at) AS INTEGER) / {bucket_seconds} * {bucket_seconds})"
            )
        }
        DatabaseBackend::MySql => {
            format!("(UNIX_TIMESTAMP({table}.at) DIV {bucket_seconds} * {bucket_seconds})")
        }
        DatabaseBackend::Postgres => {
            format!(
                "(EXTRACT(EPOCH FROM {table}.at)::bigint / {bucket_seconds} * {bucket_seconds})"
            )
        }
        _ => {
            // Fallback: use SQLite-style (works for most DBs with strftime)
            format!(
                "(CAST(strftime('%s', {table}.at) AS INTEGER) / {bucket_seconds} * {bucket_seconds})"
            )
        }
    }
}

/// Generate a positional parameter placeholder for the given backend.
/// MySQL uses `?`, SQLite and Postgres use `$N`.
fn param(backend: DatabaseBackend, n: usize) -> String {
    match backend {
        DatabaseBackend::MySql => "?".to_string(),
        _ => format!("${n}"),
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::ActiveValue::{NotSet, Set};
    use serde_json::json;
    use time::{Duration, OffsetDateTime};

    use super::*;
    use crate::seaorm::entities::{downstream_requests, providers, upstream_requests, usages};

    async fn test_storage() -> SeaOrmStorage {
        let storage = SeaOrmStorage::connect("sqlite::memory:", None)
            .await
            .expect("connect test storage");
        storage.sync().await.expect("sync test storage");
        storage
    }

    async fn insert_provider(storage: &SeaOrmStorage, name: &str, channel: &str) -> i64 {
        providers::Entity::insert(providers::ActiveModel {
            id: NotSet,
            name: Set(name.to_string()),
            channel: Set(channel.to_string()),
            label: Set(None),
            settings_json: Set(json!({})),
            dispatch_json: Set(json!({})),
            created_at: Set(OffsetDateTime::UNIX_EPOCH),
            updated_at: Set(OffsetDateTime::UNIX_EPOCH),
        })
        .exec(storage.connection())
        .await
        .expect("insert provider")
        .last_insert_id
    }

    async fn insert_downstream_request(
        storage: &SeaOrmStorage,
        at: OffsetDateTime,
        response_status: Option<i32>,
    ) {
        downstream_requests::Entity::insert(downstream_requests::ActiveModel {
            trace_id: NotSet,
            at: Set(at),
            internal: Set(false),
            user_id: Set(None),
            user_key_id: Set(None),
            request_method: Set("POST".to_string()),
            request_headers_json: Set(json!({})),
            request_path: Set("/v1/messages".to_string()),
            request_query: Set(None),
            request_body: Set(None),
            response_status: Set(response_status),
            response_headers_json: Set(json!({})),
            response_body: Set(None),
            created_at: Set(at),
        })
        .exec(storage.connection())
        .await
        .expect("insert downstream request");
    }

    async fn insert_usage(
        storage: &SeaOrmStorage,
        at: OffsetDateTime,
        provider_id: Option<i64>,
        model: Option<&str>,
        cost: f64,
        input_tokens: i64,
        output_tokens: i64,
    ) {
        usages::Entity::insert(usages::ActiveModel {
            trace_id: NotSet,
            downstream_trace_id: Set(None),
            at: Set(at),
            provider_id: Set(provider_id),
            credential_id: Set(None),
            user_id: Set(None),
            user_key_id: Set(None),
            operation: Set("chat".to_string()),
            protocol: Set("openai".to_string()),
            model: Set(model.map(str::to_string)),
            input_tokens: Set(Some(input_tokens)),
            output_tokens: Set(Some(output_tokens)),
            cache_read_input_tokens: Set(None),
            cache_creation_input_tokens: Set(None),
            cache_creation_input_tokens_5min: Set(None),
            cache_creation_input_tokens_1h: Set(None),
            cost: Set(cost),
            created_at: Set(at),
        })
        .exec(storage.connection())
        .await
        .expect("insert usage");
    }

    async fn insert_upstream_request(
        storage: &SeaOrmStorage,
        at: OffsetDateTime,
        total_latency_ms: Option<i64>,
    ) {
        upstream_requests::Entity::insert(upstream_requests::ActiveModel {
            trace_id: NotSet,
            downstream_trace_id: Set(None),
            at: Set(at),
            internal: Set(false),
            provider_id: Set(None),
            credential_id: Set(None),
            request_method: Set("POST".to_string()),
            request_headers_json: Set(json!({})),
            request_url: Set(Some("https://example.test".to_string())),
            request_body: Set(None),
            response_status: Set(Some(200)),
            response_headers_json: Set(json!({})),
            response_body: Set(None),
            initial_latency_ms: Set(None),
            total_latency_ms: Set(total_latency_ms),
            created_at: Set(at),
        })
        .exec(storage.connection())
        .await
        .expect("insert upstream request");
    }

    #[tokio::test]
    async fn overview_includes_usage_only_buckets_and_latency_kpis() {
        let storage = test_storage().await;
        let base = OffsetDateTime::UNIX_EPOCH + Duration::hours(24);

        insert_downstream_request(&storage, base, Some(200)).await;
        insert_usage(&storage, base, None, Some("gpt-4o"), 1.5, 100, 50).await;
        insert_usage(
            &storage,
            base + Duration::hours(1),
            None,
            Some("gpt-4o-mini"),
            2.0,
            80,
            20,
        )
        .await;
        insert_upstream_request(&storage, base, Some(120)).await;
        insert_upstream_request(&storage, base + Duration::minutes(5), Some(480)).await;

        let overview = storage
            .query_dashboard_overview(&DashboardQuery {
                from_unix_ms: base.unix_timestamp() * 1000,
                to_unix_ms: (base + Duration::hours(2)).unix_timestamp() * 1000,
                bucket_seconds: 3600,
            })
            .await
            .expect("query overview");

        assert_eq!(overview.kpi.total_requests, 1);
        assert_eq!(overview.kpi.success_count, 1);
        assert_eq!(overview.kpi.total_cost, 3.5);
        assert_eq!(overview.kpi.total_input_tokens, 180);
        assert_eq!(overview.kpi.total_output_tokens, 70);
        assert_eq!(overview.kpi.avg_latency_ms, Some(300.0));
        assert_eq!(overview.kpi.max_latency_ms, Some(480));
        assert_eq!(overview.status_codes.len(), 1);
        assert_eq!(overview.traffic.len(), 2);
        assert_eq!(overview.traffic[0].request_count, 1);
        assert_eq!(overview.traffic[0].cost, 1.5);
        assert_eq!(overview.traffic[1].request_count, 0);
        assert_eq!(overview.traffic[1].cost, 2.0);
    }

    #[tokio::test]
    async fn top_queries_group_by_provider_and_model() {
        let storage = test_storage().await;
        let base = OffsetDateTime::UNIX_EPOCH + Duration::hours(48);
        let openai_id = insert_provider(&storage, "openai-main", "openai").await;
        let anthropic_id = insert_provider(&storage, "claude-main", "claude").await;

        insert_usage(
            &storage,
            base,
            Some(openai_id),
            Some("gpt-4o"),
            1.0,
            100,
            20,
        )
        .await;
        insert_usage(
            &storage,
            base + Duration::minutes(1),
            Some(openai_id),
            Some("gpt-4o"),
            2.0,
            110,
            30,
        )
        .await;
        insert_usage(
            &storage,
            base + Duration::minutes(2),
            Some(anthropic_id),
            Some("claude-sonnet"),
            4.0,
            120,
            40,
        )
        .await;

        let query = DashboardQuery {
            from_unix_ms: base.unix_timestamp() * 1000,
            to_unix_ms: (base + Duration::hours(1)).unix_timestamp() * 1000,
            bucket_seconds: 3600,
        };

        let providers = storage
            .query_dashboard_top_providers(&query)
            .await
            .expect("query top providers");
        let models = storage
            .query_dashboard_top_models(&query)
            .await
            .expect("query top models");

        assert_eq!(providers.rows.len(), 2);
        assert_eq!(providers.rows[0].provider_id, Some(openai_id));
        assert_eq!(providers.rows[0].channel.as_deref(), Some("openai"));
        assert_eq!(providers.rows[0].request_count, 2);
        assert_eq!(providers.rows[0].total_cost, 3.0);

        assert_eq!(models.rows.len(), 2);
        assert_eq!(models.rows[0].model.as_deref(), Some("gpt-4o"));
        assert_eq!(models.rows[0].request_count, 2);
        assert_eq!(models.rows[0].total_cost, 3.0);
    }
}
