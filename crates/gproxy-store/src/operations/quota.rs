use crate::{
    Repository, Result, StoreError,
    entity::limits::{quota_settlement, quota_window},
    error::{invalid, receipt},
    repository::models,
};
use gproxy_seaorm::{BatchConnectionTrait, BatchStatement, FixedDecimal, SelectProjection};
use sea_orm::sea_query::{Expr, ExprTrait, OnConflict, Query, Value};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryTrait, Set};

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
