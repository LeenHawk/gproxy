use crate::{Result, Store};
use gproxy_seaorm::{BatchConnectionTrait, BatchQuery, D1Type, Projection};
use sea_orm::{DbBackend, Statement};

/// Encoded capture payload budget, independent of metadata history retention.
/// None disables that limit. In-progress captures are never pruned.
#[derive(Clone, Copy, Debug)]
pub struct PayloadRetention {
    pub days: Option<u32>,
    pub max_bytes: Option<u64>,
}
impl Default for PayloadRetention {
    fn default() -> Self {
        Self {
            days: Some(7),
            max_bytes: Some(2 * 1024 * 1024 * 1024),
        }
    }
}

/// Captures whose payload is pruned per statement batch.
const PRUNE_BATCH: u32 = 128;
/// Upper bound on measure-then-prune rounds in one budget pass. Each round
/// removes the estimated overshoot, so a second round only corrects skew in
/// the per-capture average; the next periodic pass picks up any remainder.
const BUDGET_ROUNDS: u32 = 3;

impl<C: BatchConnectionTrait> Store<C> {
    /// Capture deletion cascades through manifests and links. This sweep also
    /// handles event-only deletion, and frees blobs only after their last link.
    /// Blob FKs protect concurrent writers: a conflicting sweep fails atomically
    /// and the periodic caller retries on its next pass.
    pub async fn collect_capture_garbage(&self) -> Result<()> {
        let backend = self.db.get_database_backend();
        self.db.atomic_batch_owned(vec![
            Statement::from_string(backend, "DELETE FROM capture_bodies WHERE NOT EXISTS (SELECT 1 FROM upstream_records r WHERE r.request_body_id = capture_bodies.id) AND NOT EXISTS (SELECT 1 FROM downstream_records r WHERE r.request_body_id = capture_bodies.id) AND NOT EXISTS (SELECT 1 FROM upstream_events e WHERE e.body_id = capture_bodies.id) AND NOT EXISTS (SELECT 1 FROM downstream_events e WHERE e.body_id = capture_bodies.id)"),
            Statement::from_string(backend, "DELETE FROM capture_blobs WHERE NOT EXISTS (SELECT 1 FROM capture_body_blobs l WHERE l.blob_id = capture_blobs.id)"),
        ]).await?;
        Ok(())
    }

    /// Encoded bytes, plus chunk offset lists and manifest hash references.
    /// Database pages, indexes and metadata/header sets belong to the separate
    /// database/history budget, not this portable payload budget.
    ///
    /// This sums every payload column, so it costs a scan of the payload
    /// tables. Call it on the budget cadence, not on every maintenance tick.
    pub async fn capture_payload_bytes(&self) -> Result<u64> {
        Ok(self.capture_payload_totals().await?.0)
    }

    async fn capture_payload_totals(&self) -> Result<(u64, u64)> {
        let backend = self.db.get_database_backend();
        let length = if backend == DbBackend::Postgres {
            "octet_length"
        } else {
            "length"
        };
        let mut counts = Vec::new();
        let mut terms = vec![
            format!("(SELECT COALESCE(SUM({length}(payload)), 0) FROM capture_blobs)"),
            "(SELECT COUNT(*) * 64 FROM capture_body_blobs)".into(),
        ];
        for side in ["upstream", "downstream"] {
            counts.push(format!("(SELECT COUNT(*) FROM {side}_records r WHERE request_body IS NOT NULL OR response_body IS NOT NULL OR request_body_id IS NOT NULL OR EXISTS (SELECT 1 FROM {side}_events e WHERE e.capture_id = r.id))"));
            terms.push(format!("(SELECT COALESCE(SUM(COALESCE({length}(request_body), 0) + COALESCE({length}(response_body), 0)), 0) FROM {side}_records)"));
            terms.push(format!("(SELECT COALESCE(SUM({length}(payload) + COALESCE({length}(chunk_offsets), 0)), 0) FROM {side}_events)"));
        }
        let rows = self
            .db
            .query_rows(BatchQuery::new(
                Statement::from_string(
                    backend,
                    format!(
                        "SELECT CAST({} AS BIGINT) AS bytes, CAST({} AS BIGINT) AS captures",
                        terms.join(" + "),
                        counts.join(" + ")
                    ),
                ),
                Projection::new()
                    .column("bytes", D1Type::I64, false)?
                    .column("captures", D1Type::I64, false)?,
            ))
            .await?;
        Ok((
            rows[0].try_get::<i64>("", "bytes")?.max(0) as u64,
            rows[0].try_get::<i64>("", "captures")?.max(0) as u64,
        ))
    }

    /// Age pass followed by budget pass. Hosts with a cheap maintenance tick
    /// should call [`Self::prune_expired_capture_payloads`] on that tick and
    /// [`Self::enforce_capture_payload_budget`] less often, since the budget
    /// pass measures the payload tables.
    pub async fn prune_capture_payloads(&self, policy: PayloadRetention, now: i64) -> Result<u64> {
        let mut removed = 0;
        if let Some(days) = policy.days {
            removed += self.prune_expired_capture_payloads(days, now).await?;
        }
        if let Some(limit) = policy.max_bytes {
            removed += self.enforce_capture_payload_budget(limit, now).await?;
        }
        Ok(removed)
    }

    /// Prune completed payloads older than `days`, retaining records, headers,
    /// usage and capture_links. The lookup is an indexed range on
    /// `ended_at_ms`, committed in small batches so request writers can
    /// interleave with retention on SQLite/libSQL. Blobs are collected only
    /// when something was removed.
    pub async fn prune_expired_capture_payloads(&self, days: u32, now: i64) -> Result<u64> {
        let cutoff = now.saturating_sub(i64::from(days) * 86_400_000);
        let mut removed = 0;
        loop {
            let count = self.prune_payload_batch(cutoff, PRUNE_BATCH).await?;
            removed += count;
            if count < u64::from(PRUNE_BATCH) {
                break;
            }
        }
        if removed > 0 {
            self.collect_capture_garbage().await?;
        }
        Ok(removed)
    }

    /// Prune oldest completed payloads until the encoded total fits `limit`.
    /// Measures once per round and removes the estimated overshoot in one go,
    /// so a pass costs a bounded number of scans regardless of how far over
    /// budget the store is.
    pub async fn enforce_capture_payload_budget(&self, limit: u64, now: i64) -> Result<u64> {
        let mut removed = 0;
        for _ in 0..BUDGET_ROUNDS {
            let (bytes, captures) = self.capture_payload_totals().await?;
            if bytes <= limit || captures == 0 {
                break;
            }
            let average = (bytes / captures).max(1);
            let mut target = (bytes - limit).div_ceil(average).max(1);
            let mut progressed = false;
            while target > 0 {
                let batch = target.min(u64::from(PRUNE_BATCH)) as u32;
                let count = self.prune_payload_batch(now, batch).await?;
                removed += count;
                if count == 0 {
                    break;
                }
                progressed = true;
                target = target.saturating_sub(count);
            }
            if !progressed {
                break;
            }
            self.collect_capture_garbage().await?;
        }
        Ok(removed)
    }

    async fn prune_payload_batch(&self, cutoff: i64, limit: u32) -> Result<u64> {
        let backend = self.db.get_database_backend();
        let param = if backend == DbBackend::Postgres {
            "$1"
        } else {
            "?"
        };
        let mut selects = Vec::new();
        for side in ["upstream", "downstream"] {
            // cutoff is a typed integer, not caller SQL. Embed it once per arm
            // to keep this UNION portable to SQLite and Postgres parameter rules.
            selects.push(format!("SELECT id, '{side}' AS side, ended_at_ms FROM {side}_records r WHERE ended_at_ms <= {cutoff} AND state <> 'in_progress' AND (request_body IS NOT NULL OR response_body IS NOT NULL OR request_body_id IS NOT NULL OR EXISTS (SELECT 1 FROM {side}_events e WHERE e.capture_id = r.id))"));
        }
        let rows = self
            .db
            .query_rows(BatchQuery::new(
                Statement::from_string(
                    backend,
                    format!(
                        "{} ORDER BY ended_at_ms, side, id LIMIT {limit}",
                        selects.join(" UNION ALL ")
                    ),
                ),
                Projection::new()
                    .column("id", D1Type::Text, false)?
                    .column("side", D1Type::Text, false)?
                    .column("ended_at_ms", D1Type::I64, false)?,
            ))
            .await?;
        let mut statements = Vec::new();
        for row in &rows {
            let id: String = row.try_get("", "id")?;
            let side: String = row.try_get("", "side")?;
            // side comes exclusively from the two literal SELECT arms above.
            for sql in [
                format!("DELETE FROM {side}_events WHERE capture_id = {param}"),
                format!(
                    "UPDATE {side}_records SET request_body = NULL, response_body = NULL, request_body_id = NULL, request_body_encoding = 'identity', response_body_encoding = 'identity', request_body_state = 'not_captured', response_body_state = 'not_captured' WHERE id = {param}"
                ),
                format!("DELETE FROM capture_bodies WHERE {side}_id = {param}"),
            ] {
                statements.push(Statement::from_sql_and_values(
                    backend,
                    sql,
                    [id.clone().into()],
                ));
            }
        }
        if !statements.is_empty() {
            self.db.atomic_batch_owned(statements).await?;
        }
        Ok(rows.len() as u64)
    }
}
