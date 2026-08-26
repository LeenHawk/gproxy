use crate::backend::Statement;
use crate::query::runtime;
use crate::{Store, StoreError};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CleanupResult {
    pub retention_rows: u64,
    pub pressure_rows: u64,
    pub size_bytes: u64,
    pub over_size_limit: bool,
}

impl Store {
    pub async fn cleanup_observability(
        &self,
        retention_cutoff: Option<i64>,
        max_database_bytes: Option<u64>,
    ) -> Result<CleanupResult, StoreError> {
        let retention_rows = match retention_cutoff {
            Some(cutoff) => {
                let statements = ["wire_logs", "request_logs", "usage_rows"]
                    .into_iter()
                    .map(|table| runtime::delete_before(table, cutoff))
                    .collect::<Result<Vec<_>, _>>()?;
                affected(self.backend().batch(statements).await?)
            }
            None => 0,
        };
        let size_bytes = database_size_bytes(self).await?;
        let over_size_limit = max_database_bytes.is_some_and(|limit| size_bytes > limit);
        let pressure_rows = if over_size_limit {
            affected(self.backend().batch(runtime::delete_oldest_logs()?).await?)
        } else {
            0
        };
        Ok(CleanupResult {
            retention_rows,
            pressure_rows,
            size_bytes,
            over_size_limit,
        })
    }
}

async fn database_size_bytes(store: &Store) -> Result<u64, StoreError> {
    let results = store
        .backend()
        .batch(vec![
            Statement::plain("PRAGMA page_count"),
            Statement::plain("PRAGMA page_size"),
        ])
        .await?;
    let page_count = pragma_value(results.first(), "page_count")?;
    let page_size = pragma_value(results.get(1), "page_size")?;
    page_count
        .checked_mul(page_size)
        .ok_or_else(|| StoreError::InvalidData {
            field: "database_size",
            message: "database page size overflow".into(),
        })
}

fn pragma_value(
    result: Option<&crate::backend::QueryResult>,
    column: &'static str,
) -> Result<u64, StoreError> {
    let value = result
        .and_then(|result| result.rows.first())
        .ok_or_else(|| StoreError::Database(format!("{column} returned no row")))?
        .i64(column)?;
    u64::try_from(value).map_err(|error| StoreError::InvalidData {
        field: column,
        message: error.to_string(),
    })
}

fn affected(results: Vec<crate::backend::QueryResult>) -> u64 {
    results.into_iter().map(|result| result.affected_rows).sum()
}
