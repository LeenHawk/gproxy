use crate::{
    Repository, Result, StoreError,
    entity::resource::protocol_state,
    error::receipt,
    repository::{affected, models},
};
use gproxy_seaorm::{BatchConnectionTrait, BatchStatement, SelectProjection};
use sea_orm::sea_query::{Expr, OnConflict};
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, QueryTrait, Set};

#[derive(Clone, Debug)]
pub struct StateValue {
    pub payload: Vec<u8>,
    pub expires_at_ms: Option<i64>,
}
#[derive(Clone, Debug)]
pub struct StateChange {
    pub scope: String,
    pub key: String,
    pub expected: Option<Vec<u8>>,
    pub replacement: Option<StateValue>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StateOutcome {
    Applied(Option<Vec<u8>>),
    Conflict,
}

fn live(now: i64) -> Condition {
    Condition::any()
        .add(protocol_state::Column::ExpiresAtMs.is_null())
        .add(protocol_state::Column::ExpiresAtMs.gt(now))
}

impl<C: BatchConnectionTrait> Repository<'_, C, protocol_state::Entity> {
    pub async fn get_live_many(
        &self,
        keys: &[(String, String)],
        now: i64,
    ) -> Result<Vec<Option<protocol_state::Model>>> {
        let queries = keys
            .iter()
            .map(|id| protocol_state::Entity::find_by_id(id.clone()).filter(live(now)))
            .collect();
        Ok(self
            .query_many(queries)
            .await?
            .into_iter()
            .map(|mut rows| rows.pop())
            .collect())
    }
    pub async fn compare_exchange_many(
        &self,
        changes: Vec<StateChange>,
        now: i64,
    ) -> Result<Vec<StateOutcome>> {
        let backend = self.db.get_database_backend();
        let mut batch = Vec::new();
        let mut outcomes = Vec::new();
        for change in changes {
            let id = (change.scope.clone(), change.key.clone());
            let selected = protocol_state::Entity::find_by_id(id.clone());
            if let Some(value) = change.replacement {
                let next = receipt()?;
                let statement = if let Some(expected) = change.expected {
                    protocol_state::Entity::update_many()
                        .col_expr(protocol_state::Column::Version, Expr::val(next.clone()))
                        .col_expr(protocol_state::Column::Payload, Expr::val(value.payload))
                        .col_expr(
                            protocol_state::Column::ExpiresAtMs,
                            Expr::val(value.expires_at_ms),
                        )
                        .filter(protocol_state::Column::Scope.eq(change.scope))
                        .filter(protocol_state::Column::Key.eq(change.key))
                        .filter(protocol_state::Column::Version.eq(expected))
                        .filter(live(now))
                        .build(backend)
                } else {
                    let expired = protocol_state::Column::ExpiresAtMs.lte(now);
                    // Expiry is assigned last so MySQL's left-to-right UPDATE
                    // assignment evaluation uses the same old expiry for all fields.
                    let conflict = OnConflict::columns([
                        protocol_state::Column::Scope,
                        protocol_state::Column::Key,
                    ])
                    .value(
                        protocol_state::Column::Version,
                        Expr::case(expired.clone(), Expr::val(next.clone()))
                            .finally(Expr::col(protocol_state::Column::Version)),
                    )
                    .value(
                        protocol_state::Column::Payload,
                        Expr::case(expired.clone(), Expr::val(value.payload.clone()))
                            .finally(Expr::col(protocol_state::Column::Payload)),
                    )
                    .value(
                        protocol_state::Column::ExpiresAtMs,
                        Expr::case(expired, Expr::val(value.expires_at_ms))
                            .finally(Expr::col(protocol_state::Column::ExpiresAtMs)),
                    )
                    .to_owned();
                    protocol_state::Entity::insert(protocol_state::ActiveModel {
                        scope: Set(change.scope),
                        key: Set(change.key),
                        version: Set(next.clone()),
                        payload: Set(value.payload),
                        expires_at_ms: Set(value.expires_at_ms),
                    })
                    .on_conflict(conflict)
                    .build(backend)
                };
                batch.push(BatchStatement::Execute(statement));
                batch.push(BatchStatement::Query(
                    selected
                        .filter(protocol_state::Column::Version.eq(next.clone()))
                        .batch_query(backend)?,
                ));
                outcomes.push(Some(next));
            } else if let Some(expected) = change.expected {
                batch.push(BatchStatement::Execute(
                    protocol_state::Entity::delete_by_id(id)
                        .filter(protocol_state::Column::Version.eq(expected))
                        .filter(live(now))
                        .build(backend),
                ));
                outcomes.push(None);
            } else {
                batch.push(BatchStatement::Execute(
                    protocol_state::Entity::delete_by_id(id)
                        .filter(protocol_state::Column::ExpiresAtMs.lte(now))
                        .build(backend),
                ));
                batch.push(BatchStatement::Query(
                    selected.filter(live(now)).batch_query(backend)?,
                ));
                outcomes.push(Some(Vec::new()));
            }
        }
        let mut results = self.db.batch(&batch).await?.into_iter();
        outcomes
            .into_iter()
            .map(|next| {
                let rows = affected(results.next().ok_or(StoreError::UnexpectedResult)?)?;
                match next {
                    None => Ok(if rows > 0 {
                        StateOutcome::Applied(None)
                    } else {
                        StateOutcome::Conflict
                    }),
                    Some(version) => {
                        let present = !models::<protocol_state::Model>(
                            results.next().ok_or(StoreError::UnexpectedResult)?,
                        )?
                        .is_empty();
                        Ok(if version.is_empty() {
                            if present {
                                StateOutcome::Conflict
                            } else {
                                StateOutcome::Applied(None)
                            }
                        } else if present {
                            StateOutcome::Applied(Some(version))
                        } else {
                            StateOutcome::Conflict
                        })
                    }
                }
            })
            .collect()
    }
}
