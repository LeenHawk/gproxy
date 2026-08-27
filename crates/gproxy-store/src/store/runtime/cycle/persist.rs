use crate::backend::{QueryResult, Statement};
use crate::query::runtime;
use crate::records::CredentialQuotaObservation;
use crate::{Store, StoreError};

use super::metrics::Collected;

pub(super) async fn known(
    store: &Store,
    cycle: Statement,
    cycle_id: i64,
    version: u64,
    metrics: &Collected,
) -> Result<QueryResult, StoreError> {
    let mut statements = Vec::with_capacity(metrics.models.len() + 2);
    statements.push(cycle);
    statements.push(runtime::delete_credential_cycle_models(cycle_id, version)?);
    statements.extend(
        metrics
            .models
            .iter()
            .map(|model| runtime::insert_credential_cycle_model(cycle_id, version, model))
            .collect::<Result<Vec<_>, _>>()?,
    );
    first(store.backend().batch(statements).await?)
}

pub(super) async fn insert(
    store: &Store,
    cycle: Statement,
    input: &CredentialQuotaObservation,
    metrics: &Collected,
) -> Result<QueryResult, StoreError> {
    let mut statements = Vec::with_capacity(metrics.models.len() + 1);
    statements.push(cycle);
    statements.extend(
        metrics
            .models
            .iter()
            .map(|model| runtime::insert_open_credential_cycle_model(input, model))
            .collect::<Result<Vec<_>, _>>()?,
    );
    first(store.backend().batch(statements).await?)
}

fn first(mut results: Vec<QueryResult>) -> Result<QueryResult, StoreError> {
    if results.is_empty() {
        Err(StoreError::Database(
            "credential cycle transaction returned no result".into(),
        ))
    } else {
        Ok(results.remove(0))
    }
}
