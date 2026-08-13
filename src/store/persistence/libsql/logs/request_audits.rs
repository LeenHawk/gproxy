//! Logical request-audit view over the existing downstream/upstream wire logs.

use serde_json::Value;

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{col_bool, col_i64, col_opt_i64, col_str};
use crate::store::persistence::libsql::util::{query, query_one};
use crate::store::persistence::records::RequestAudit;
use crate::store::persistence::{LogQuery, PageQuery, PageResult};

fn grouped_sql(q: &LogQuery) -> (String, Vec<Value>) {
    let mut sql = String::from(
        "SELECT request_id, audit_at AS at, method, target, status, audit_provider_id AS provider_id, \
                upstream_attempts, has_downstream FROM (\
           SELECT request_events.*, \
                  MAX(at) OVER (PARTITION BY request_id) AS audit_at, \
                  SUM(is_upstream) OVER (PARTITION BY request_id) AS upstream_attempts, \
                  MAX(has_downstream) OVER (PARTITION BY request_id) AS has_downstream, \
                  FIRST_VALUE(provider_id) OVER (\
                    PARTITION BY request_id \
                    ORDER BY is_upstream DESC, at DESC, event_id DESC\
                  ) AS audit_provider_id, \
                  ROW_NUMBER() OVER (\
                    PARTITION BY request_id \
                    ORDER BY has_downstream DESC, at DESC, event_id DESC\
                  ) AS row_number \
           FROM (\
           SELECT id AS event_id, request_id, at, method, path AS target, status, \
                  NULL AS provider_id, CAST(0 AS BIGINT) AS is_upstream, \
                  CAST(1 AS BIGINT) AS has_downstream \
                  FROM downstream_requests \
           UNION ALL \
           SELECT id AS event_id, request_id, at, method, url AS target, status, \
                  provider_id, CAST(1 AS BIGINT) AS is_upstream, \
                  CAST(0 AS BIGINT) AS has_downstream \
                  FROM upstream_requests\
         ) request_events\
         ) ranked WHERE row_number = 1",
    );
    let mut args = Vec::new();
    if let Some(v) = q.at_from {
        sql.push_str(" AND audit_at >= ?");
        args.push(arg_integer(v));
    }
    if let Some(v) = q.at_to {
        sql.push_str(" AND audit_at <= ?");
        args.push(arg_integer(v));
    }
    if let Some(v) = q.provider_id {
        sql.push_str(" AND (request_id IN (SELECT request_id FROM upstream_requests WHERE provider_id = ?) OR request_id IN (SELECT request_id FROM usages WHERE provider_id = ?))");
        args.push(arg_integer(v));
        args.push(arg_integer(v));
    }
    if let Some(v) = q.user_id {
        sql.push_str(" AND request_id IN (SELECT request_id FROM usages WHERE user_id = ?)");
        args.push(arg_integer(v));
    }
    if let Some(ref v) = q.route_name {
        sql.push_str(" AND request_id IN (SELECT request_id FROM usages WHERE route_name = ?)");
        args.push(arg_text(v));
    }
    (sql, args)
}

pub async fn query_page(
    client: &LibsqlClient,
    q: &LogQuery,
    page: &PageQuery,
) -> anyhow::Result<PageResult<RequestAudit>> {
    let (grouped, args) = grouped_sql(q);
    let count = query_one(
        client,
        &format!("SELECT COUNT(*) FROM ({grouped}) grouped"),
        &args,
    )
    .await?
    .ok_or_else(|| anyhow::anyhow!("request audit count returned no row"))?;
    let total = u64::try_from(col_i64(&count, 0)?)?;
    let mut page_args = args;
    page_args.push(arg_integer(i64::try_from(page.limit)?));
    page_args.push(arg_integer(i64::try_from(page.offset)?));
    let rows = query(
        client,
        &format!(
            "SELECT * FROM ({grouped}) grouped ORDER BY at DESC, request_id DESC LIMIT ? OFFSET ?"
        ),
        &page_args,
    )
    .await?;
    let items = rows
        .iter()
        .map(|row| {
            Ok(RequestAudit {
                request_id: col_str(row, 0)?,
                at: col_i64(row, 1)?,
                method: col_str(row, 2)?,
                target: col_str(row, 3)?,
                status: col_i64(row, 4)?,
                provider_id: col_opt_i64(row, 5)?,
                upstream_attempts: col_i64(row, 6)?,
                has_downstream: col_bool(row, 7)?,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(PageResult { items, total })
}
