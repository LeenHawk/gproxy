use rust_decimal::Decimal;

use super::{boundary, row};
use crate::query::runtime;
use crate::records::{CredentialQuotaCycleRecord, CredentialQuotaPressure};
use crate::{Store, StoreError};

impl Store {
    pub async fn open_credential_quota_cycles(
        &self,
        credential_id: i64,
        now: i64,
    ) -> Result<Vec<CredentialQuotaCycleRecord>, StoreError> {
        self.query_open_credential_quota_cycles(Some(credential_id), now)
            .await
    }

    pub async fn credential_quota_cycle_history(
        &self,
        credential_id: i64,
        window_key: &str,
    ) -> Result<Vec<CredentialQuotaCycleRecord>, StoreError> {
        self.backend()
            .execute(runtime::select_credential_quota_cycle_history(
                credential_id,
                window_key,
            )?)
            .await?
            .rows
            .into_iter()
            .map(row::parse)
            .collect()
    }

    pub async fn credential_quota_pressures(
        &self,
        now: i64,
    ) -> Result<Vec<CredentialQuotaPressure>, StoreError> {
        Ok(self
            .query_open_credential_quota_cycles(None, now)
            .await?
            .into_iter()
            .filter_map(|cycle| {
                pressure(&cycle).map(|used_percent| CredentialQuotaPressure {
                    cycle_id: cycle.id,
                    credential_id: cycle.credential_id,
                    window_key: cycle.window_key.clone(),
                    version: cycle.version,
                    last_observed_at: cycle.last_observed_at,
                    used_percent,
                    period_end: boundary::trusted_reset(&cycle),
                })
            })
            .collect())
    }

    pub async fn credential_quota_pressure(
        &self,
        credential_id: i64,
        now: i64,
    ) -> Result<Option<Decimal>, StoreError> {
        Ok(self
            .open_credential_quota_cycles(credential_id, now)
            .await?
            .iter()
            .filter_map(pressure)
            .max())
    }

    async fn query_open_credential_quota_cycles(
        &self,
        credential_id: Option<i64>,
        now: i64,
    ) -> Result<Vec<CredentialQuotaCycleRecord>, StoreError> {
        self.backend()
            .execute(runtime::select_open_credential_quota_cycles(credential_id)?)
            .await?
            .rows
            .into_iter()
            .map(row::parse)
            .filter_map(|result| match result {
                Ok(cycle) if boundary::trusted_reset(&cycle).is_some_and(|reset| reset <= now) => {
                    None
                }
                result => Some(result),
            })
            .collect()
    }

    pub(super) async fn open_credential_quota_cycle(
        &self,
        credential_id: i64,
        window_key: &str,
    ) -> Result<Option<CredentialQuotaCycleRecord>, StoreError> {
        let result = self
            .backend()
            .execute(runtime::read_open_credential_quota_cycle(
                credential_id,
                window_key,
            )?)
            .await?;
        result.rows.into_iter().next().map(row::parse).transpose()
    }

    pub(super) async fn credential_quota_cycle(
        &self,
        id: i64,
    ) -> Result<Option<CredentialQuotaCycleRecord>, StoreError> {
        let result = self
            .backend()
            .execute(runtime::read_credential_quota_cycle(id)?)
            .await?;
        result.rows.into_iter().next().map(row::parse).transpose()
    }

    pub(super) async fn latest_credential_quota_cycle(
        &self,
        credential_id: i64,
        window_key: &str,
    ) -> Result<Option<CredentialQuotaCycleRecord>, StoreError> {
        let result = self
            .backend()
            .execute(runtime::read_latest_credential_quota_cycle(
                credential_id,
                window_key,
            )?)
            .await?;
        result.rows.into_iter().next().map(row::parse).transpose()
    }

    pub(super) async fn require_credential_quota_cycle(
        &self,
        id: i64,
    ) -> Result<CredentialQuotaCycleRecord, StoreError> {
        self.credential_quota_cycle(id)
            .await?
            .ok_or_else(|| StoreError::Database("credential quota cycle vanished".into()))
    }
}

fn pressure(cycle: &CredentialQuotaCycleRecord) -> Option<Decimal> {
    cycle.used_percent.or_else(|| {
        let limit = cycle.upstream_limit?;
        let used = cycle.upstream_used?;
        (limit > Decimal::ZERO).then(|| used / limit * Decimal::ONE_HUNDRED)
    })
}
