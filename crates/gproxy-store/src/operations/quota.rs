use crate::{
    Repository, Result, StoreError,
    entity::limits::{quota_settlement, quota_window},
    error::{invalid, receipt},
    repository::models,
};
use gproxy_seaorm::{BatchConnectionTrait, BatchStatement, FixedDecimal, SelectProjection};
use sea_orm::sea_query::{Expr, ExprTrait, OnConflict, Query, Value};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QueryTrait, Set};

#[derive(Clone, Debug)]
pub struct Settlement {
    pub window_id: String,
    pub request_id: String,
    pub amount: FixedDecimal,
    pub settled_at_ms: i64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettlementOutcome {
    Applied,
    AlreadySettled,
}

impl<C: BatchConnectionTrait> Repository<'_, C, quota_settlement::Entity> {
    /// Persist contributions and increment windows in one transaction. Exact
    /// replays do not charge twice; a different amount under the same key fails
    /// its NOT NULL constraint and rolls back the batch. Counters never overflow
    /// to SQLite REAL. Fresh receipts are generated per transmission attempt.
    pub async fn settle_many(&self, entries: Vec<Settlement>) -> Result<Vec<SettlementOutcome>> {
        let backend = self.db.get_database_backend();
        let mut batch = Vec::new();
        let count = entries.len();
        for entry in entries {
            if entry.amount.atoms() < 0 {
                return Err(invalid("settlement amount must be nonnegative"));
            }
            let tag = receipt()?;
            let old = Expr::col((quota_settlement::Entity, quota_settlement::Column::Amount));
            let value = quota_settlement::Column::Amount.save_as(Expr::val(entry.amount));
            // The conflict branch keeps the first receipt/timestamp. NULL in the
            // required amount column makes incompatible retries fail atomically.
            let same =
                Expr::case(old.clone().eq(value), old).finally(Expr::val(Value::BigInt(None)));
            let conflict = OnConflict::columns([
                quota_settlement::Column::WindowId,
                quota_settlement::Column::RequestId,
            ])
            .value(quota_settlement::Column::Amount, same)
            .to_owned();
            let insert = quota_settlement::Entity::insert(quota_settlement::ActiveModel {
                window_id: Set(entry.window_id.clone()),
                request_id: Set(entry.request_id.clone()),
                amount: Set(entry.amount),
                settled_at_ms: Set(entry.settled_at_ms),
                receipt: Set(tag.clone()),
            })
            .on_conflict(conflict)
            .build(backend);
            batch.push(BatchStatement::Execute(insert));
            let proof = quota_settlement::Entity::find_by_id((
                entry.window_id.clone(),
                entry.request_id.clone(),
            ))
            .filter(quota_settlement::Column::Receipt.eq(tag));
            let bound = FixedDecimal::from_atoms(i64::MAX - entry.amount.atoms());
            let next = Expr::col(quota_window::Column::Used)
                .add(quota_window::Column::Used.save_as(Expr::val(entry.amount)));
            let guarded = Expr::case(
                sea_orm::Condition::all()
                    .add(quota_window::Column::Used.gte(FixedDecimal::ZERO))
                    .add(quota_window::Column::Used.lte(bound)),
                next,
            )
            .finally(Expr::val(Value::BigInt(None)));
            let update = Query::update()
                .table(quota_window::Entity)
                .value(quota_window::Column::Used, guarded)
                .and_where(quota_window::Column::Id.eq(entry.window_id))
                .and_where(Expr::exists(proof.clone().into_query()))
                .to_owned();
            batch.push(BatchStatement::Execute(backend.build(&update)));
            batch.push(BatchStatement::Query(proof.batch_query(backend)?));
        }
        let mut results = self.db.batch(&batch).await?.into_iter();
        (0..count)
            .map(|_| {
                results.next().ok_or(StoreError::UnexpectedResult)?;
                results.next().ok_or(StoreError::UnexpectedResult)?;
                let applied = !models::<quota_settlement::Model>(
                    results.next().ok_or(StoreError::UnexpectedResult)?,
                )?
                .is_empty();
                Ok(if applied {
                    SettlementOutcome::Applied
                } else {
                    SettlementOutcome::AlreadySettled
                })
            })
            .collect()
    }
}

/// A budget window to open for `(quota_id, starts_at_ms)`. Opening is
/// idempotent: an existing row with the same key is returned untouched, so
/// concurrent instances converge on one window.
#[derive(Clone, Debug)]
pub struct OpenWindow {
    pub quota_id: String,
    pub starts_at_ms: i64,
    /// None for a permanent (`total`) window.
    pub ends_at_ms: Option<i64>,
    /// The quota row as configured when the window opened.
    pub quota_snapshot: serde_json::Value,
}

impl<C: BatchConnectionTrait> Repository<'_, C, quota_window::Entity> {
    /// Open each window unless one with the same `(quota_id, starts_at_ms)`
    /// exists, and return the row that stands afterwards. Ids are random;
    /// the unique `window` key is the identity that matters.
    pub async fn open_many(&self, windows: Vec<OpenWindow>) -> Result<Vec<quota_window::Model>> {
        let backend = self.db.get_database_backend();
        let mut batch = Vec::with_capacity(windows.len() * 2);
        for window in &windows {
            let insert = quota_window::Entity::insert(quota_window::ActiveModel {
                id: Set(hex(&receipt()?)),
                quota_id: Set(window.quota_id.clone()),
                starts_at_ms: Set(window.starts_at_ms),
                ends_at_ms: Set(window.ends_at_ms),
                used: Set(FixedDecimal::ZERO),
                quota_snapshot: Set(window.quota_snapshot.clone()),
            })
            .on_conflict(
                OnConflict::columns([
                    quota_window::Column::QuotaId,
                    quota_window::Column::StartsAtMs,
                ])
                .do_nothing_on([quota_window::Column::Id])
                .to_owned(),
            )
            .build(backend);
            batch.push(BatchStatement::Execute(insert));
            batch.push(BatchStatement::Query(
                quota_window::Entity::find()
                    .filter(quota_window::Column::QuotaId.eq(window.quota_id.clone()))
                    .filter(quota_window::Column::StartsAtMs.eq(window.starts_at_ms))
                    .batch_query(backend)?,
            ));
        }
        let mut results = self.db.batch(&batch).await?.into_iter();
        windows
            .iter()
            .map(|_| {
                results.next().ok_or(StoreError::UnexpectedResult)?;
                models::<quota_window::Model>(results.next().ok_or(StoreError::UnexpectedResult)?)?
                    .pop()
                    .ok_or(StoreError::UnexpectedResult)
            })
            .collect()
    }

    /// Windows of `quota_ids` that contain `now_ms`: started, and either
    /// permanent or not yet ended. Ordered by quota id then start.
    pub async fn open_at(
        &self,
        quota_ids: &[String],
        now_ms: i64,
    ) -> Result<Vec<quota_window::Model>> {
        if quota_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.query(
            quota_window::Entity::find()
                .filter(quota_window::Column::QuotaId.is_in(quota_ids.to_vec()))
                .filter(quota_window::Column::StartsAtMs.lte(now_ms))
                .filter(
                    sea_orm::Condition::any()
                        .add(quota_window::Column::EndsAtMs.is_null())
                        .add(quota_window::Column::EndsAtMs.gt(now_ms)),
                )
                .order_by_asc(quota_window::Column::QuotaId)
                .order_by_asc(quota_window::Column::StartsAtMs),
        )
        .await
    }

    /// End the named windows at `ends_at_ms`. History is kept: the rows and
    /// their settlements stay in place.
    pub async fn close_many(&self, window_ids: &[String], ends_at_ms: i64) -> Result<()> {
        if window_ids.is_empty() {
            return Ok(());
        }
        self.update_many(
            window_ids
                .iter()
                .map(|id| quota_window::ActiveModel {
                    id: Set(id.clone()),
                    ends_at_ms: Set(Some(ends_at_ms)),
                    ..Default::default()
                })
                .collect(),
        )
        .await?;
        Ok(())
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
