use crate::query::usage;
use crate::records::{UsageFilter, UsageRecord, UsageTotals};
use crate::{Store, StoreError};

impl Store {
    pub async fn active_usage_credentials(
        &self,
        since: i64,
    ) -> Result<std::collections::BTreeSet<i64>, StoreError> {
        self.backend()
            .execute(usage::active_credentials(since)?)
            .await?
            .rows
            .into_iter()
            .map(|row| row.i64("credential_id"))
            .collect()
    }
    pub async fn usage_records(
        &self,
        filter: &UsageFilter,
        page: u64,
        page_size: u64,
    ) -> Result<(Vec<UsageRecord>, u64), StoreError> {
        let offset = page
            .checked_sub(1)
            .and_then(|page| page.checked_mul(page_size))
            .filter(|_| matches!(page_size, 10 | 20 | 50 | 100))
            .ok_or_else(|| StoreError::InvalidData {
                field: "pagination",
                message: "invalid page or page size".into(),
            })?;
        let mut results = self
            .backend()
            .batch(vec![
                usage::records(filter, offset, page_size)?,
                usage::count_filtered(filter)?,
            ])
            .await?
            .into_iter();
        let rows = results
            .next()
            .expect("records result")
            .rows
            .into_iter()
            .map(super::usage::parse_usage)
            .collect::<Result<Vec<_>, _>>()?;
        let count = results
            .next()
            .expect("count result")
            .rows
            .pop()
            .expect("count row")
            .i64("count")? as u64;
        Ok((rows, count))
    }

    pub async fn usage_summary(&self, filter: &UsageFilter) -> Result<UsageTotals, StoreError> {
        let mut totals = UsageTotals::default();
        let mut after = 0;
        loop {
            let rows = self
                .backend()
                .execute(usage::summary_rows(filter, after, 5_000)?)
                .await?
                .rows;
            if rows.is_empty() {
                break;
            }
            for row in rows {
                let record = super::usage::parse_usage(row)?;
                after = record.id;
                totals.add(&record.usage)?;
            }
        }
        Ok(totals)
    }
}
