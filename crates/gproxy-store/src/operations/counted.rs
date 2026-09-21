use crate::{
    Repository, Result, StoreError,
    entity::limits::counted_window,
    repository::{affected, models},
};
use gproxy_seaorm::{BatchConnectionTrait, BatchStatement, SelectProjection};
use sea_orm::sea_query::{Expr, ExprTrait, OnConflict};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryTrait, Set};

/// One charge against a Counted dimension's current window. The charge is
/// applied only when it fits under `limit`; the window row is created on
/// first use.
#[derive(Clone, Debug)]
pub struct CountedCharge {
    pub credential_id: String,
    pub dimension: String,
    pub window_start_ms: i64,
    pub window_end_ms: i64,
    pub amount: i64,
    pub limit: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CountedOutcome {
    /// Whether this charge was added.
    pub applied: bool,
    /// The window's usage after this call.
    pub used: i64,
}

impl<C: BatchConnectionTrait> Repository<'_, C, counted_window::Entity> {
    /// Add each charge to its window when `used + amount <= limit`, atomically
    /// per row. A refused charge leaves the row untouched; the outcome carries
    /// the usage either way so the caller can block the credential.
    pub async fn charge_many(&self, charges: Vec<CountedCharge>) -> Result<Vec<CountedOutcome>> {
        let backend = self.db.get_database_backend();
        let mut batch = Vec::with_capacity(charges.len() * 3);
        for charge in &charges {
            let id = (
                charge.credential_id.clone(),
                charge.dimension.clone(),
                charge.window_start_ms,
            );
            batch.push(BatchStatement::Query(
                counted_window::Entity::find_by_id(id.clone()).batch_query(backend)?,
            ));
            let fits = Expr::col(counted_window::Column::Used)
                .add(Expr::val(charge.amount))
                .lte(Expr::col(counted_window::Column::Limit));
            let conflict = OnConflict::columns([
                counted_window::Column::CredentialId,
                counted_window::Column::Dimension,
                counted_window::Column::WindowStartMs,
            ])
            .value(
                counted_window::Column::Used,
                Expr::case(
                    fits,
                    Expr::col(counted_window::Column::Used).add(Expr::val(charge.amount)),
                )
                .finally(Expr::col(counted_window::Column::Used)),
            )
            .to_owned();
            // The first charge of a window is admitted when it fits alone.
            let initial = if charge.amount <= charge.limit {
                charge.amount
            } else {
                0
            };
            batch.push(BatchStatement::Execute(
                counted_window::Entity::insert(counted_window::ActiveModel {
                    credential_id: Set(charge.credential_id.clone()),
                    dimension: Set(charge.dimension.clone()),
                    window_start_ms: Set(charge.window_start_ms),
                    window_end_ms: Set(charge.window_end_ms),
                    used: Set(initial),
                    limit: Set(charge.limit),
                })
                .on_conflict(conflict)
                .build(backend),
            ));
            batch.push(BatchStatement::Query(
                counted_window::Entity::find_by_id(id).batch_query(backend)?,
            ));
        }
        let mut results = self.db.batch(&batch).await?.into_iter();
        charges
            .iter()
            .map(|charge| {
                let before = models::<counted_window::Model>(
                    results.next().ok_or(StoreError::UnexpectedResult)?,
                )?
                .pop()
                .map_or(0, |row| row.used);
                affected(results.next().ok_or(StoreError::UnexpectedResult)?)?;
                let after = models::<counted_window::Model>(
                    results.next().ok_or(StoreError::UnexpectedResult)?,
                )?
                .pop()
                .map_or(0, |row| row.used);
                Ok(CountedOutcome {
                    applied: after == before + charge.amount,
                    used: after,
                })
            })
            .collect()
    }

    /// Windows still open at `now_ms`, for reloading availability.
    pub async fn live(&self, now_ms: i64) -> Result<Vec<counted_window::Model>> {
        self.query(
            counted_window::Entity::find().filter(counted_window::Column::WindowEndMs.gt(now_ms)),
        )
        .await
    }
}
