//! Configuration writes that advance `settings.config_revision`.
//!
//! Every management write is one batch: the caller's statements, then the
//! revision bump, then a read of the new value. Peers learn about the change by
//! comparing this number, so a write that lands without a bump would be
//! invisible to them and one that bumps without landing would trigger a pointless
//! reload. Both are impossible here because the batch is atomic.

use crate::{
    Result, Store, StoreError, entity::config::setting, error::invalid, repository::models,
};
use gproxy_seaorm::{BatchConnectionTrait, BatchResult, BatchStatement, SelectProjection};
use sea_orm::sea_query::{Expr, ExprTrait, Query};
use sea_orm::{ColumnTrait, EntityTrait};

/// The durable revision after the write, and one result per caller statement
/// in the order the statements were given. The bump and the read-back are not
/// included.
#[derive(Debug)]
pub struct Commit {
    pub revision: i64,
    pub results: Vec<BatchResult>,
}

impl<C: BatchConnectionTrait> Store<C> {
    /// Run `statements`, increment `config_revision` and read it back, all in
    /// one transaction. A failing statement rolls the bump back with it, so
    /// the revision only ever moves for a write that actually happened.
    ///
    /// The settings row must exist: `Settings::update` (or `sync`) creates it.
    /// Its absence is reported as an invalid request, but the caller's
    /// statements have committed by then — the batch has no row to fail on.
    pub async fn commit_revision(&self, statements: Vec<BatchStatement>) -> Result<Commit> {
        let backend = self.db.get_database_backend();
        let mut batch = statements;
        let caller = batch.len();
        let bump = Query::update()
            .table(setting::Entity)
            .value(
                setting::Column::ConfigRevision,
                Expr::col(setting::Column::ConfigRevision).add(1),
            )
            .and_where(setting::Column::Id.eq(setting::GLOBAL_SETTINGS_ID))
            .to_owned();
        batch.push(BatchStatement::Execute(backend.build(&bump)));
        batch.push(BatchStatement::Query(
            setting::Entity::find_by_id(setting::GLOBAL_SETTINGS_ID).batch_query(backend)?,
        ));
        let mut results = self.db.batch(&batch).await?;
        if results.len() != caller + 2 {
            return Err(StoreError::UnexpectedResult);
        }
        let read = results.pop().ok_or(StoreError::UnexpectedResult)?;
        results.pop();
        let revision = models::<setting::Model>(read)?
            .into_iter()
            .next()
            .ok_or_else(|| invalid("global settings row is missing"))?
            .config_revision;
        Ok(Commit { revision, results })
    }
}
