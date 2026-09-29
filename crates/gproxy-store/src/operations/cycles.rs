//! Credential cycles: open, accrue, observe, close. Core decides what happens
//! to a cycle; these are the statements that make it happen without losing a
//! concurrent writer's work — accrual is `cost_usd = cost_usd + ?` in the
//! database, every transition is guarded on the cycle still being open, and
//! opening meets a concurrent opener on the unique `open_key`.

use crate::{
    Repository, Result, StoreError,
    entity::limits::credential_cycle::{self, CycleBoundary, CycleOpening},
    error::invalid,
    repository::{affected, models},
};
use gproxy_seaorm::{BatchConnectionTrait, BatchStatement, FixedDecimal, SelectProjection};
use sea_orm::sea_query::{Expr, ExprTrait, OnConflict, Query};
use sea_orm::{
    ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder, QuerySelect, QueryTrait, Set,
};

/// One upstream reading, stored on the cycle as a set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CycleSample {
    pub used_percent: Option<FixedDecimal>,
    pub used: Option<FixedDecimal>,
    pub limit: Option<FixedDecimal>,
    pub at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct NewCycle {
    pub id: String,
    pub credential_id: String,
    pub window_id: String,
    pub dimension_id: Option<String>,
    pub scope: serde_json::Value,
    pub starts_at_ms: i64,
    pub ends_at_ms: Option<i64>,
    pub boundary: CycleBoundary,
    pub opened_by: CycleOpening,
    /// Cost it opens with: zero, or what a backfill moved into it.
    pub cost_usd: FixedDecimal,
    pub sample: Option<CycleSample>,
}

#[derive(Clone, Debug)]
pub enum CycleChange {
    /// Close an open cycle at `at_ms`, moving `moved` of its cost out (what a
    /// server-reset backfill found was spent after the split). The cost never
    /// goes below zero.
    Close {
        id: String,
        at_ms: i64,
        moved: FixedDecimal,
    },
    /// Open a cycle unless its window already has an open one.
    Open(NewCycle),
    /// Adopt boundaries and a reading on an open cycle. The sample's cost is
    /// the cycle's `cost_usd` at the moment the statement runs.
    Observe {
        id: String,
        starts_at_ms: i64,
        ends_at_ms: Option<i64>,
        boundary: CycleBoundary,
        sample: Option<CycleSample>,
    },
}

impl<C: BatchConnectionTrait> Repository<'_, C, credential_cycle::Entity> {
    /// The open cycles of `credential_ids`, by credential then window.
    pub async fn open_of(&self, credential_ids: &[String]) -> Result<Vec<credential_cycle::Model>> {
        if credential_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.query(
            credential_cycle::Entity::find()
                .filter(credential_cycle::Column::CredentialId.is_in(credential_ids.to_vec()))
                .filter(credential_cycle::Column::ClosedAtMs.is_null())
                .order_by_asc(credential_cycle::Column::CredentialId)
                .order_by_asc(credential_cycle::Column::WindowId),
        )
        .await
    }

    /// Every open cycle of one credential and at most `closed` of its most
    /// recently closed ones, in one snapshot: open first by window, then
    /// closed newest first.
    pub async fn recent(
        &self,
        credential_id: &str,
        closed: u64,
    ) -> Result<Vec<credential_cycle::Model>> {
        let open = credential_cycle::Entity::find()
            .filter(credential_cycle::Column::CredentialId.eq(credential_id))
            .filter(credential_cycle::Column::ClosedAtMs.is_null())
            .order_by_asc(credential_cycle::Column::WindowId);
        let history = credential_cycle::Entity::find()
            .filter(credential_cycle::Column::CredentialId.eq(credential_id))
            .filter(credential_cycle::Column::ClosedAtMs.is_not_null())
            .order_by_desc(credential_cycle::Column::ClosedAtMs)
            .order_by_desc(credential_cycle::Column::Id)
            .limit(Ord::max(closed, 1));
        let mut sets = self.query_many(vec![open, history]).await?.into_iter();
        let mut rows = sets.next().unwrap_or_default();
        if closed > 0 {
            rows.extend(sets.next().unwrap_or_default());
        }
        Ok(rows)
    }

    /// Every open cycle of one credential and, for every window it ever had,
    /// at most `closed_per_window` of that window's most recently closed
    /// cycles: open first by window, then closed by window, newest first.
    ///
    /// Bounded per window rather than overall, because windows close at very
    /// different rates: a total cap would fill with 5-hour cycles and leave
    /// the weekly window no history at all. Two round trips — the window ids
    /// first, then one bounded read per window in a single batch.
    pub async fn history(
        &self,
        credential_id: &str,
        closed_per_window: u64,
    ) -> Result<Vec<credential_cycle::Model>> {
        let backend = self.db.get_database_backend();
        let windows = credential_cycle::Entity::find()
            .select_only()
            .column(credential_cycle::Column::WindowId)
            .distinct()
            .filter(credential_cycle::Column::CredentialId.eq(credential_id))
            .filter(credential_cycle::Column::ClosedAtMs.is_not_null())
            .order_by_asc(credential_cycle::Column::WindowId)
            .batch_query(backend)?;
        let mut sets = self.db.query_batch(&[windows]).await?.into_iter();
        let window_ids = sets
            .next()
            .ok_or(StoreError::UnexpectedResult)?
            .iter()
            .map(|row| row.try_get::<String>("", "window_id").map_err(Into::into))
            .collect::<Result<Vec<_>>>()?;
        let mut queries = vec![
            credential_cycle::Entity::find()
                .filter(credential_cycle::Column::CredentialId.eq(credential_id))
                .filter(credential_cycle::Column::ClosedAtMs.is_null())
                .order_by_asc(credential_cycle::Column::WindowId),
        ];
        if closed_per_window > 0 {
            queries.extend(window_ids.into_iter().map(|window_id| {
                credential_cycle::Entity::find()
                    .filter(credential_cycle::Column::CredentialId.eq(credential_id))
                    .filter(credential_cycle::Column::WindowId.eq(window_id))
                    .filter(credential_cycle::Column::ClosedAtMs.is_not_null())
                    .order_by_desc(credential_cycle::Column::ClosedAtMs)
                    .order_by_desc(credential_cycle::Column::Id)
                    .limit(closed_per_window)
            }));
        }
        Ok(self
            .query_many(queries)
            .await?
            .into_iter()
            .flatten()
            .collect())
    }

    /// Add `amount` to each of `ids` that is still open, atomically per row.
    /// `false` marks a cycle that closed under the caller, whose share the
    /// caller must place again.
    pub async fn accrue(&self, ids: &[String], amount: FixedDecimal) -> Result<Vec<bool>> {
        if amount.atoms() < 0 {
            return Err(invalid("cycle accrual must be nonnegative"));
        }
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let backend = self.db.get_database_backend();
        let batch = ids
            .iter()
            .map(|id| {
                let next = Expr::col(credential_cycle::Column::CostUsd)
                    .add(credential_cycle::Column::CostUsd.save_as(Expr::val(amount)));
                BatchStatement::Execute(
                    backend.build(
                        Query::update()
                            .table(credential_cycle::Entity)
                            .value(credential_cycle::Column::CostUsd, next)
                            .and_where(credential_cycle::Column::Id.eq(id))
                            .and_where(credential_cycle::Column::ClosedAtMs.is_null()),
                    ),
                )
            })
            .collect::<Vec<_>>();
        self.db
            .batch(&batch)
            .await?
            .into_iter()
            .map(|result| Ok(affected(result)? > 0))
            .collect()
    }

    /// Apply `changes` in order, in one transaction, then read the
    /// credential's open cycles back in the same round trip. Returns the rows
    /// each change affected — zero for a close or an observe of a cycle that
    /// was no longer open, or an open whose window already had one — and the
    /// open cycles afterwards.
    pub async fn apply(
        &self,
        credential_id: &str,
        changes: Vec<CycleChange>,
    ) -> Result<(Vec<u64>, Vec<credential_cycle::Model>)> {
        let backend = self.db.get_database_backend();
        let count = changes.len();
        let mut batch = Vec::with_capacity(count + 1);
        for change in changes {
            let statement = match change {
                CycleChange::Close { id, at_ms, moved } => {
                    if moved.atoms() < 0 {
                        return Err(invalid("moved cycle cost must be nonnegative"));
                    }
                    let cost = Expr::col(credential_cycle::Column::CostUsd);
                    let moved = credential_cycle::Column::CostUsd.save_as(Expr::val(moved));
                    let remaining = Expr::case(cost.clone().gte(moved.clone()), cost.sub(moved))
                        .finally(
                            credential_cycle::Column::CostUsd
                                .save_as(Expr::val(FixedDecimal::ZERO)),
                        );
                    backend.build(
                        Query::update()
                            .table(credential_cycle::Entity)
                            .value(credential_cycle::Column::ClosedAtMs, at_ms)
                            .value(credential_cycle::Column::OpenKey, Option::<String>::None)
                            .value(credential_cycle::Column::CostUsd, remaining)
                            .and_where(credential_cycle::Column::Id.eq(id))
                            .and_where(credential_cycle::Column::ClosedAtMs.is_null()),
                    )
                }
                CycleChange::Open(cycle) => {
                    if cycle.credential_id != credential_id {
                        return Err(invalid("a cycle change names another credential"));
                    }
                    let sample = cycle.sample.clone();
                    credential_cycle::Entity::insert(credential_cycle::ActiveModel {
                        credential_id: Set(cycle.credential_id.clone()),
                        closed_at_ms: Set(None),
                        id: Set(cycle.id),
                        open_key: Set(Some(credential_cycle::open_key(
                            &cycle.credential_id,
                            &cycle.window_id,
                        ))),
                        window_id: Set(cycle.window_id),
                        dimension_id: Set(cycle.dimension_id),
                        scope: Set(cycle.scope),
                        starts_at_ms: Set(cycle.starts_at_ms),
                        ends_at_ms: Set(cycle.ends_at_ms),
                        boundary: Set(cycle.boundary),
                        opened_by: Set(cycle.opened_by),
                        cost_usd: Set(cycle.cost_usd),
                        sample_used_percent: Set(sample.as_ref().and_then(|s| s.used_percent)),
                        sample_used: Set(sample.as_ref().and_then(|s| s.used)),
                        sample_limit: Set(sample.as_ref().and_then(|s| s.limit)),
                        sample_cost_usd: Set(sample.as_ref().map(|_| cycle.cost_usd)),
                        sample_at_ms: Set(sample.as_ref().map(|s| s.at_ms)),
                    })
                    .on_conflict(
                        OnConflict::column(credential_cycle::Column::OpenKey)
                            .do_nothing_on([credential_cycle::Column::Id])
                            .to_owned(),
                    )
                    .build(backend)
                }
                CycleChange::Observe {
                    id,
                    starts_at_ms,
                    ends_at_ms,
                    boundary,
                    sample,
                } => {
                    let mut update = Query::update();
                    update
                        .table(credential_cycle::Entity)
                        .value(credential_cycle::Column::StartsAtMs, starts_at_ms)
                        .value(credential_cycle::Column::EndsAtMs, ends_at_ms)
                        .value(credential_cycle::Column::Boundary, boundary)
                        .and_where(credential_cycle::Column::Id.eq(id))
                        .and_where(credential_cycle::Column::ClosedAtMs.is_null());
                    if let Some(sample) = sample {
                        let decimal =
                            |column: credential_cycle::Column, value: Option<FixedDecimal>| {
                                column.save_as(Expr::val(value))
                            };
                        update
                            .value(
                                credential_cycle::Column::SampleUsedPercent,
                                decimal(
                                    credential_cycle::Column::SampleUsedPercent,
                                    sample.used_percent,
                                ),
                            )
                            .value(
                                credential_cycle::Column::SampleUsed,
                                decimal(credential_cycle::Column::SampleUsed, sample.used),
                            )
                            .value(
                                credential_cycle::Column::SampleLimit,
                                decimal(credential_cycle::Column::SampleLimit, sample.limit),
                            )
                            .value(
                                credential_cycle::Column::SampleCostUsd,
                                Expr::col(credential_cycle::Column::CostUsd),
                            )
                            .value(credential_cycle::Column::SampleAtMs, sample.at_ms);
                    }
                    backend.build(&update)
                }
            };
            batch.push(BatchStatement::Execute(statement));
        }
        batch.push(BatchStatement::Query(
            credential_cycle::Entity::find()
                .filter(
                    Condition::all()
                        .add(credential_cycle::Column::CredentialId.eq(credential_id))
                        .add(credential_cycle::Column::ClosedAtMs.is_null()),
                )
                .order_by_asc(credential_cycle::Column::WindowId)
                .batch_query(backend)?,
        ));
        let mut results = self.db.batch(&batch).await?.into_iter();
        let affected_rows = (0..count)
            .map(|_| affected(results.next().ok_or(StoreError::UnexpectedResult)?))
            .collect::<Result<Vec<_>>>()?;
        let open =
            models::<credential_cycle::Model>(results.next().ok_or(StoreError::UnexpectedResult)?)?;
        Ok((affected_rows, open))
    }
}
