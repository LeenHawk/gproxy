//! Logical request-audit view over the existing downstream/upstream wire logs.

use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement, Value};

use crate::store::persistence::records::RequestAudit;
use crate::store::persistence::{LogQuery, PageQuery, PageResult};

fn bind(sql: &mut String, values: &mut Vec<Value>, backend: DatabaseBackend, value: Value) {
    let placeholder = if backend == DatabaseBackend::Postgres {
        format!("${}", values.len() + 1)
    } else {
        "?".to_owned()
    };
    sql.push_str(&placeholder);
    values.push(value);
}

fn grouped_sql(q: &LogQuery, backend: DatabaseBackend) -> (String, Vec<Value>) {
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
    let mut values = Vec::new();
    if let Some(v) = q.at_from {
        sql.push_str(" AND audit_at >= ");
        bind(&mut sql, &mut values, backend, v.into());
    }
    if let Some(v) = q.at_to {
        sql.push_str(" AND audit_at <= ");
        bind(&mut sql, &mut values, backend, v.into());
    }
    if let Some(v) = q.provider_id {
        sql.push_str(
            " AND (request_id IN (SELECT request_id FROM upstream_requests WHERE provider_id = ",
        );
        bind(&mut sql, &mut values, backend, v.into());
        sql.push_str(") OR request_id IN (SELECT request_id FROM usages WHERE provider_id = ");
        bind(&mut sql, &mut values, backend, v.into());
        sql.push_str("))");
    }
    if let Some(v) = q.user_id {
        sql.push_str(" AND request_id IN (SELECT request_id FROM usages WHERE user_id = ");
        bind(&mut sql, &mut values, backend, v.into());
        sql.push(')');
    }
    if let Some(ref v) = q.route_name {
        sql.push_str(" AND request_id IN (SELECT request_id FROM usages WHERE route_name = ");
        bind(&mut sql, &mut values, backend, v.clone().into());
        sql.push(')');
    }
    (sql, values)
}

pub async fn query_page(
    conn: &DatabaseConnection,
    q: &LogQuery,
    page: &PageQuery,
) -> anyhow::Result<PageResult<RequestAudit>> {
    let backend = conn.get_database_backend();
    let (grouped, values) = grouped_sql(q, backend);
    let count_sql = format!("SELECT CAST(COUNT(*) AS BIGINT) AS total FROM ({grouped}) grouped");
    let count = conn
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            count_sql,
            values.clone(),
        ))
        .await?
        .ok_or_else(|| anyhow::anyhow!("request audit count returned no row"))?;
    let total = u64::try_from(count.try_get::<i64>("", "total")?)?;

    let mut page_sql =
        format!("SELECT * FROM ({grouped}) grouped ORDER BY at DESC, request_id DESC LIMIT ");
    let mut page_values = values;
    bind(
        &mut page_sql,
        &mut page_values,
        backend,
        i64::try_from(page.limit)?.into(),
    );
    page_sql.push_str(" OFFSET ");
    bind(
        &mut page_sql,
        &mut page_values,
        backend,
        i64::try_from(page.offset)?.into(),
    );
    let items = conn
        .query_all_raw(Statement::from_sql_and_values(
            backend,
            page_sql,
            page_values,
        ))
        .await?
        .into_iter()
        .map(|row| {
            Ok(RequestAudit {
                request_id: row.try_get("", "request_id")?,
                at: row.try_get("", "at")?,
                method: row.try_get("", "method")?,
                target: row.try_get("", "target")?,
                status: row.try_get("", "status")?,
                provider_id: row.try_get("", "provider_id")?,
                upstream_attempts: row.try_get("", "upstream_attempts")?,
                has_downstream: row.try_get::<i64>("", "has_downstream")? != 0,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(PageResult { items, total })
}
