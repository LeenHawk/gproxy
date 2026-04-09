use sea_orm::*;
use sea_orm::sea_query::Expr;

use super::helpers::{apply_desc_cursor, unix_ms_to_offset_datetime};
use crate::query::*;
use crate::seaorm::SeaOrmStorage;
use crate::seaorm::entities::*;

/// Internal helper that applies the shared `UsageQuery` filter conditions to
/// any select statement implementing `QueryFilter`.
///
/// Works with both `Select<usages::Entity>` and
/// `SelectTwo<usages::Entity, providers::Entity>` so the same filter logic can
/// power `query_usages`, `count_usages`, and `summarize_usages` without
/// duplication.
///
/// `requires_provider_join` should be `true` when the caller has already
/// joined `providers` (so we can filter by `providers.channel`); when `false`
/// the channel filter is silently skipped because the join would otherwise
/// have to be added speculatively.
fn apply_usage_filters<S>(mut select: S, query: &UsageQuery, requires_provider_join: bool) -> S
where
    S: QueryFilter,
{
    if let Scope::Eq(ref v) = query.provider_id {
        select = select.filter(usages::Column::ProviderId.eq(*v));
    }
    if let Scope::Eq(ref v) = query.credential_id {
        select = select.filter(usages::Column::CredentialId.eq(*v));
    }
    if requires_provider_join {
        if let Scope::Eq(ref v) = query.channel {
            select = select.filter(providers::Column::Channel.eq(v.clone()));
        }
    }
    if let Scope::Eq(ref v) = query.model {
        select = select.filter(usages::Column::Model.eq(v.clone()));
    }
    if let Scope::Eq(ref v) = query.user_id {
        select = select.filter(usages::Column::UserId.eq(*v));
    }
    if let Scope::Eq(ref v) = query.user_key_id {
        select = select.filter(usages::Column::UserKeyId.eq(*v));
    }
    if let Some(from) = query.from_unix_ms {
        select = select.filter(usages::Column::At.gte(unix_ms_to_offset_datetime(from)));
    }
    if let Some(to) = query.to_unix_ms {
        select = select.filter(usages::Column::At.lte(unix_ms_to_offset_datetime(to)));
    }
    select
}

/// Whether the query has any conditions that require joining `providers`.
fn usage_query_needs_provider_join(query: &UsageQuery) -> bool {
    matches!(query.channel, Scope::Eq(_))
}

/// Usage queries — always hit the database at runtime.
/// Usage records are not cached in memory (too large).
impl SeaOrmStorage {
    pub async fn query_usages(&self, query: &UsageQuery) -> Result<Vec<UsageQueryRow>, DbErr> {
        let mut select = usages::Entity::find()
            .find_also_related(providers::Entity)
            .order_by_desc(usages::Column::At)
            .order_by_desc(usages::Column::TraceId);

        // `find_also_related` already produces the LEFT JOIN against
        // `providers`, so the channel filter can run unconditionally.
        select = apply_usage_filters(select, query, true);
        select = apply_desc_cursor(
            select,
            query.cursor_at_unix_ms,
            query.cursor_trace_id,
            usages::Column::At,
            usages::Column::TraceId,
        );
        if let Some(offset) = query.offset {
            select = select.offset(offset);
        }
        if let Some(limit) = query.limit {
            select = select.limit(limit);
        }

        let rows = select.all(&self.db).await?;
        Ok(rows
            .into_iter()
            .map(|(r, provider)| UsageQueryRow {
                trace_id: r.trace_id,
                downstream_trace_id: r.downstream_trace_id,
                at: r.at,
                provider_id: r.provider_id,
                provider_channel: provider.map(|p| p.channel),
                credential_id: r.credential_id,
                user_id: r.user_id,
                user_key_id: r.user_key_id,
                operation: r.operation,
                protocol: r.protocol,
                model: r.model,
                input_tokens: r.input_tokens,
                output_tokens: r.output_tokens,
                cache_read_input_tokens: r.cache_read_input_tokens,
                cache_creation_input_tokens: r.cache_creation_input_tokens,
                cache_creation_input_tokens_5min: r.cache_creation_input_tokens_5min,
                cache_creation_input_tokens_1h: r.cache_creation_input_tokens_1h,
            })
            .collect())
    }

    pub async fn count_usages(&self, query: &UsageQuery) -> Result<UsageQueryCount, DbErr> {
        let needs_join = usage_query_needs_provider_join(query);
        let mut select = usages::Entity::find();
        if needs_join {
            select = select.join(JoinType::LeftJoin, usages::Relation::Providers.def());
        }
        select = apply_usage_filters(select, query, needs_join);
        let count = select.count(&self.db).await?;
        Ok(UsageQueryCount { count })
    }

    /// Aggregate usage rows matching `query` into running totals across the
    /// full result set (not just the current page). Used by the admin
    /// dashboard's metric cards and the per-user "my usage" summary so the
    /// numbers reflect lifetime totals rather than the page slice.
    pub async fn summarize_usages(&self, query: &UsageQuery) -> Result<UsageSummary, DbErr> {
        #[derive(FromQueryResult, Default)]
        struct Row {
            count: i64,
            input_tokens: Option<i64>,
            output_tokens: Option<i64>,
            cache_read_input_tokens: Option<i64>,
            cache_creation_input_tokens: Option<i64>,
            cache_creation_input_tokens_5min: Option<i64>,
            cache_creation_input_tokens_1h: Option<i64>,
        }

        let needs_join = usage_query_needs_provider_join(query);
        let mut select = usages::Entity::find().select_only();
        if needs_join {
            select = select.join(JoinType::LeftJoin, usages::Relation::Providers.def());
        }
        select = select
            .column_as(Expr::col(usages::Column::TraceId).count(), "count")
            .column_as(Expr::col(usages::Column::InputTokens).sum(), "input_tokens")
            .column_as(
                Expr::col(usages::Column::OutputTokens).sum(),
                "output_tokens",
            )
            .column_as(
                Expr::col(usages::Column::CacheReadInputTokens).sum(),
                "cache_read_input_tokens",
            )
            .column_as(
                Expr::col(usages::Column::CacheCreationInputTokens).sum(),
                "cache_creation_input_tokens",
            )
            .column_as(
                Expr::col(usages::Column::CacheCreationInputTokens5min).sum(),
                "cache_creation_input_tokens_5min",
            )
            .column_as(
                Expr::col(usages::Column::CacheCreationInputTokens1h).sum(),
                "cache_creation_input_tokens_1h",
            );
        select = apply_usage_filters(select, query, needs_join);

        let row = select
            .into_model::<Row>()
            .one(&self.db)
            .await?
            .unwrap_or_default();

        Ok(UsageSummary {
            count: u64::try_from(row.count).unwrap_or(0),
            input_tokens: row.input_tokens.unwrap_or(0),
            output_tokens: row.output_tokens.unwrap_or(0),
            cache_read_input_tokens: row.cache_read_input_tokens.unwrap_or(0),
            cache_creation_input_tokens: row.cache_creation_input_tokens.unwrap_or(0),
            cache_creation_input_tokens_5min: row.cache_creation_input_tokens_5min.unwrap_or(0),
            cache_creation_input_tokens_1h: row.cache_creation_input_tokens_1h.unwrap_or(0),
        })
    }
}
