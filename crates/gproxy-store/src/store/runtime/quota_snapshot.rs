use gproxy_core::channel_api::{QuotaEntry, QuotaRefreshError, QuotaSnapshot, QuotaSourceState};
use std::collections::BTreeMap;

use crate::query::runtime::{quota_response, quota_snapshot};
use crate::{Store, StoreError};

impl Store {
    pub async fn credential_quota_snapshot(
        &self,
        credential_id: i64,
    ) -> Result<QuotaSnapshot, StoreError> {
        let mut results = self
            .backend()
            .batch(vec![
                quota_snapshot::select(credential_id)?,
                quota_response::select(credential_id)?,
            ])
            .await?;
        let responses = results.pop().expect("quota response rows").rows;
        let rows = results.pop().expect("quota source rows").rows;
        let mut snapshot = QuotaSnapshot::default();
        let mut positions = BTreeMap::new();
        for row in rows {
            snapshot.sources.push(QuotaSourceState {
                capability: deserialize(row.text("capability_json")?)?,
                attempted_at_ms: row.optional_i64("attempted_at_ms")?,
                observed_at_ms: row.optional_i64("observed_at_ms")?,
                reset_credits: row
                    .optional_text("reset_credits_json")?
                    .map(deserialize)
                    .transpose()?,
                error: row
                    .optional_text("error_code")?
                    .map(|code| {
                        Ok::<_, StoreError>(QuotaRefreshError {
                            code: code.to_owned(),
                            message: row.text("error_message")?.to_owned(),
                        })
                    })
                    .transpose()?,
            });
            for entry in deserialize::<Vec<QuotaEntry>>(row.text("entries_json")?)? {
                merge_entry(&mut snapshot.entries, &mut positions, entry);
            }
        }
        for row in responses {
            merge_entry(
                &mut snapshot.entries,
                &mut positions,
                deserialize(row.text("entry_json")?)?,
            );
        }
        Ok(snapshot)
    }

    pub async fn save_credential_quota_source(
        &self,
        credential_id: i64,
        expected_version: u64,
        state: &QuotaSourceState,
        entries: Option<&[QuotaEntry]>,
    ) -> Result<(), StoreError> {
        if entries.is_some() == state.error.is_some()
            || (entries.is_some() && state.observed_at_ms.is_none())
        {
            return Err(StoreError::InvalidData {
                field: "quota result",
                message:
                    "success requires entries and an observation time; failure requires an error"
                        .into(),
            });
        }
        if entries.is_some_and(|entries| {
            entries
                .iter()
                .any(|entry| entry.source_id != state.capability.id)
        }) {
            return Err(StoreError::InvalidData {
                field: "quota source",
                message: "entry belongs to a different source".into(),
            });
        }
        self.backend()
            .batch(quota_snapshot::save(
                credential_id,
                expected_version,
                state,
                entries,
            )?)
            .await?;
        Ok(())
    }

    pub async fn observe_credential_quota_entries(
        &self,
        credential_id: i64,
        expected_version: u64,
        entries: &[QuotaEntry],
    ) -> Result<(), StoreError> {
        let mut statements = Vec::new();
        for entry in entries {
            statements.extend(quota_response::observe(
                credential_id,
                expected_version,
                entry,
            )?);
        }
        if !statements.is_empty() {
            self.backend().batch(statements).await?;
        }
        Ok(())
    }
}

fn merge_entry(
    entries: &mut Vec<QuotaEntry>,
    positions: &mut BTreeMap<(String, String), usize>,
    entry: QuotaEntry,
) {
    let key = (entry.source_id.clone(), entry.id.clone());
    if let Some(&index) = positions.get(&key) {
        // Probe rows are merged first and win ties with response observations.
        if entry.observed_at_ms > entries[index].observed_at_ms {
            entries[index] = entry;
        }
    } else {
        positions.insert(key, entries.len());
        entries.push(entry);
    }
}

fn deserialize<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, StoreError> {
    serde_json::from_str(value).map_err(|error| StoreError::InvalidData {
        field: "quota snapshot",
        message: error.to_string(),
    })
}
