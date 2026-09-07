use gproxy_store::records::{CaptureInput, RequestLogImportInput, RequestLogInput};
use tokio_rusqlite::rusqlite::{Connection, Result, Row};

use super::super::model::{Legacy, SourceData};
use super::optional_json;

pub(in crate::migrate_v2) fn inspect(connection: &Connection, data: &mut SourceData) -> Result<()> {
    if exists(connection, "downstream_requests")? {
        let mut after = None;
        loop {
            let rows = downstream(connection, after)?;
            let Some(last) = rows.last() else { break };
            after = Some(last.id);
            data.downstream_requests += rows.len();
        }
        let mut duplicates = connection.prepare(
            "SELECT MIN(id) FROM downstream_requests GROUP BY request_id HAVING COUNT(*) > 1",
        )?;
        for id in duplicates.query_map([], |row| row.get::<_, i64>(0))? {
            data.table_issues.push(super::super::plan::issue(
                "downstream_requests",
                id?,
                "duplicate request_id cannot be represented by v3 request logs",
            ));
        }
    }
    if exists(connection, "upstream_requests")? {
        let mut after = None;
        loop {
            let rows = upstream(connection, after)?;
            let Some(last) = rows.last() else { break };
            after = Some(last.id);
            data.upstream_requests += rows.len();
            data.log_references.extend(
                rows.into_iter()
                    .map(|row| (row.value.provider_id, row.value.credential_id)),
            );
        }
    }
    Ok(())
}

pub(in crate::migrate_v2) fn downstream(
    connection: &Connection,
    after: Option<i64>,
) -> Result<Vec<Legacy<RequestLogImportInput>>> {
    let mut query = connection.prepare(
        "SELECT id,request_id,at,method,path,query,status,headers_json,body,response_body FROM downstream_requests WHERE (?1 IS NULL OR id>?1) ORDER BY id LIMIT 128",
    )?;
    query
        .query_map([after], |row| {
            Ok(Legacy {
                id: row.get(0)?,
                value: RequestLogImportInput {
                    request: RequestLogInput {
                        request_id: row.get(1)?,
                        at: row.get(2)?,
                        method: row.get(3)?,
                        path: row.get(4)?,
                        query: row.get(5)?,
                        client_ip: None,
                        request_headers: optional_json(row, 7)?,
                        request_body: body(row, 8)?,
                    },
                    response_status: status(row, 6)?,
                    response_body: body(row, 9)?,
                },
            })
        })?
        .collect()
}

pub(in crate::migrate_v2) fn upstream(
    connection: &Connection,
    after: Option<i64>,
) -> Result<Vec<Legacy<CaptureInput>>> {
    let mut query = connection.prepare(
        "SELECT id,request_id,at,provider_id,credential_id,url,method,status,headers_json,body,response_body FROM upstream_requests WHERE (?1 IS NULL OR id>?1) ORDER BY id LIMIT 128",
    )?;
    query
        .query_map([after], |row| {
            Ok(Legacy {
                id: row.get(0)?,
                value: CaptureInput {
                    request_id: row.get(1)?,
                    at: row.get(2)?,
                    provider_id: row.get(3)?,
                    credential_id: row.get(4)?,
                    upstream_url: row.get(5)?,
                    request_method: row.get(6)?,
                    response_status: status(row, 7)?,
                    request_headers: optional_json(row, 8)?,
                    request_body: body(row, 9)?,
                    response_body: body(row, 10)?,
                    response_headers: None,
                },
            })
        })?
        .collect()
}

fn exists(connection: &Connection, table: &str) -> Result<bool> {
    connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |row| row.get(0),
    )
}

fn body(row: &Row<'_>, index: usize) -> Result<Option<Vec<u8>>> {
    Ok(row.get::<_, Option<String>>(index)?.map(String::into_bytes))
}

fn status(row: &Row<'_>, index: usize) -> Result<Option<u16>> {
    let value: u16 = row.get(index)?;
    // v2 uses zero when no HTTP response was recorded.
    if value == 0 {
        return Ok(None);
    }
    http::StatusCode::from_u16(value)
        .map(|status| Some(status.as_u16()))
        .map_err(|error| {
            tokio_rusqlite::rusqlite::Error::FromSqlConversionFailure(
                index,
                tokio_rusqlite::rusqlite::types::Type::Integer,
                Box::new(error),
            )
        })
}
