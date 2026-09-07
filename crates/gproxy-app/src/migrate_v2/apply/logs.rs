use std::path::Path;

use gproxy_store::records::RecordBatch;
use tokio_rusqlite::rusqlite::OpenFlags;

use super::{Context, mark, optional};
use crate::migrate_v2::{report::ImportCount, source::logs};

pub(super) async fn run(
    context: &Context<'_>,
    source: &Path,
    counts: &mut [ImportCount],
) -> Result<(), crate::AppError> {
    let connection =
        tokio_rusqlite::Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .await
            .map_err(error)?;
    for entity in ["downstream_requests", "upstream_requests"] {
        if !counts
            .iter()
            .any(|count| count.entity == entity && count.found > 0)
        {
            continue;
        }
        let mut after = None;
        let mut imported = 0;
        loop {
            let batch = match entity {
                "downstream_requests" => {
                    let rows = connection
                        .call(move |connection| logs::downstream(connection, after))
                        .await
                        .map_err(error)?;
                    let Some(last) = rows.last() else { break };
                    after = Some(last.id);
                    RecordBatch::RequestLogs(rows.into_iter().map(|row| row.value).collect())
                }
                _ => {
                    let rows = connection
                        .call(move |connection| logs::upstream(connection, after))
                        .await
                        .map_err(error)?;
                    let Some(last) = rows.last() else { break };
                    after = Some(last.id);
                    let inputs = rows
                        .into_iter()
                        .map(|row| {
                            let mut input = row.value;
                            input.provider_id = optional(&context.providers, input.provider_id)?;
                            input.credential_id =
                                optional(&context.credentials, input.credential_id)?;
                            Ok(input)
                        })
                        .collect::<Result<Vec<_>, crate::AppError>>()?;
                    RecordBatch::Captures(inputs)
                }
            };
            imported += context.store.insert_record_batch(batch).await?.len();
        }
        mark(counts, entity, imported);
    }
    Ok(())
}

fn error(error: impl std::fmt::Display) -> crate::AppError {
    crate::AppError::Migration(format!("could not import v2 request logs: {error}"))
}
