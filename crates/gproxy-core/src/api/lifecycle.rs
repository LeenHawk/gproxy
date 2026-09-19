use super::{CoreError, CoreResult, CredentialSummary, ReloadOutcome};
use crate::keys;
use crate::{Core, CoreData, CredentialVersion};
use gproxy_seaorm::BatchConnectionTrait;
use std::sync::Arc;

/// Seven days; blocks live longer than this are re-warmed on reload.
const MAX_BLOCK_CACHE_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1000;

pub(crate) fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

impl<C: BatchConnectionTrait> Core<C> {
    /// Read provider execution configuration from this Core's Store and assemble
    /// a new snapshot without publishing it. Rows and durable revision come from
    /// one `load_control_data` batch. Providers, credentials, connection data,
    /// model metadata, endpoint overrides and compiled rewrite sets only; no
    /// routing/identity/policy/pricing tables and no schema sync. Live
    /// credential blocks are warmed into the cache as a side effect so the
    /// snapshot can be served immediately after publication.
    pub async fn load_data(&self) -> CoreResult<Arc<CoreData>> {
        let control = self.store.load_control_data().await?;
        let previous = self.snapshot();
        let now = now_ms();
        let vocabularies = self.load_vocabularies(&control, &previous).await?;
        let assembly = crate::assemble::assemble(
            &control,
            &self.channels,
            self.codec.as_ref(),
            &self.clients,
            vocabularies,
            Some(&previous),
            now,
        )
        .await?;
        for (credential_id, blocks) in assembly.blocks {
            let Some(credential) = assembly.data.credentials.get(&credential_id) else {
                continue;
            };
            self.warm_blocks(&credential.provider_id, &credential_id, blocks, now)
                .await?;
        }
        Ok(Arc::new(assembly.data))
    }

    /// Custom vocabularies referenced by the catalog or the Setting default,
    /// parsed once: a file already parsed for the previous snapshot is
    /// reused, the rest are read from file storage. Missing storage or an
    /// unreadable file leaves that vocabulary out; counting then falls back
    /// to the bundled encoders.
    async fn load_vocabularies(
        &self,
        control: &gproxy_store::ControlData,
        previous: &CoreData,
    ) -> CoreResult<crate::assemble::VocabularyMap> {
        let mut wanted: Vec<String> = control
            .models
            .iter()
            .filter_map(|m| m.vocabulary_file_id.clone())
            .chain(
                control
                    .settings
                    .as_ref()
                    .and_then(|s| s.default_vocabulary_file_id.clone()),
            )
            .collect();
        wanted.sort();
        wanted.dedup();
        let mut out = std::collections::HashMap::new();
        let mut missing = Vec::new();
        for id in wanted {
            match previous
                .estimation
                .as_ref()
                .and_then(|e| e.vocabulary(&id).cloned())
            {
                Some(vocabulary) => {
                    out.insert(id, vocabulary);
                }
                None => missing.push(id),
            }
        }
        if missing.is_empty() {
            return Ok(out);
        }
        let Some(files) = self.file_storage() else {
            return Ok(out);
        };
        let rows = self.store.file_objects().get_many(&missing).await?;
        for row in rows.into_iter().flatten() {
            let Ok(bytes) = files.read(&row.object_key).await else {
                continue;
            };
            if let Ok(vocabulary) = gproxy_tokenizer::Vocabulary::from_bytes(&bytes.to_bytes()) {
                out.insert(row.id, vocabulary);
            }
        }
        Ok(out)
    }

    /// Load/assemble through Store, then monotonically publish. Failure leaves
    /// the previous snapshot active; existing requests retain their pinned view.
    pub async fn reload_data(&self) -> CoreResult<ReloadOutcome> {
        let next = self.load_data().await?;
        let loaded_revision = next.revision;
        let published = self.publish_snapshot(next);
        Ok(ReloadOutcome {
            loaded_revision,
            active_revision: self.snapshot().revision,
            published,
        })
    }

    /// Reload persisted credential material after a peer's update, without an
    /// upstream refresh. Input order and duplicates are preserved. A row that
    /// is missing in Store returns None and retires the local slot. A row that
    /// exists but is not in the active snapshot (created after the last reload)
    /// is reported but not installed: run `reload_data` for that. Never returns
    /// secret contents. Caller is the trusted upper layer, responsible for IDs.
    pub async fn reload_credentials(
        &self,
        credential_ids: &[String],
    ) -> CoreResult<Vec<Option<CredentialSummary>>> {
        let rows = self.store.credentials().get_many(credential_ids).await?;
        let snapshot = self.snapshot();
        let mut out = Vec::with_capacity(rows.len());
        for (id, row) in credential_ids.iter().zip(rows) {
            let slot = snapshot.credentials.get(id);
            let Some(row) = row else {
                if let Some(slot) = slot {
                    slot.state.retire();
                }
                out.push(None);
                continue;
            };
            if let Some(slot) = slot {
                let secret = self
                    .codec
                    .open(&row.id, &row.secret)
                    .map_err(CoreError::Secret)?;
                slot.state.publish_if_newer(Arc::new(CredentialVersion {
                    version: row.version,
                    expires_at_ms: row.expires_at_ms,
                    secret,
                    status: row.status,
                    status_reason: row.status_reason.clone(),
                }));
            }
            out.push(Some(CredentialSummary {
                credential_id: row.id,
                version: row.version,
                expires_at_ms: row.expires_at_ms,
                status: row.status,
                status_reason: row.status_reason,
            }));
        }
        Ok(out)
    }
}

impl<C> Core<C> {
    pub(crate) async fn warm_blocks(
        &self,
        provider_id: &str,
        credential_id: &str,
        blocks: Vec<crate::CredentialBlock>,
        now_ms: i64,
    ) -> CoreResult<()> {
        warm_blocks(&self.cache, provider_id, credential_id, blocks, now_ms).await
    }
}

/// Merge persisted blocks into the cache copy with a CAS loop. The TTL is
/// the longest remaining block; readers still compare `until_ms`.
pub(crate) async fn warm_blocks(
    cache: &Arc<dyn gproxy_cache::Cache>,
    provider_id: &str,
    credential_id: &str,
    blocks: Vec<crate::CredentialBlock>,
    now_ms: i64,
) -> CoreResult<()> {
    use gproxy_cache::{CasOutcome, Replacement};
    let key = keys::credential_blocks(provider_id, credential_id);
    loop {
        let entry = cache.get(&key).await?;
        let (expected, mut current) = match &entry {
            Some(entry) => (
                Some(entry.version),
                serde_json::from_slice::<crate::CredentialBlocks>(&entry.value).unwrap_or_default(),
            ),
            None => (None, crate::CredentialBlocks::default()),
        };
        for block in blocks.iter().cloned() {
            current.upsert(block, now_ms);
        }
        let Some(until) = current.blocks.iter().map(|b| b.until_ms).max() else {
            return Ok(());
        };
        // Cache entries are a hot copy, not the record: cap the TTL so a
        // far-future block still expires from the cache and is re-warmed
        // from Store by the next reload. Readers compare `until_ms` anyway.
        let ttl = std::time::Duration::from_millis(
            u64::try_from(until - now_ms)
                .unwrap_or(1)
                .clamp(1, MAX_BLOCK_CACHE_TTL_MS),
        );
        let value = serde_json::to_vec(&current).map_err(|e| CoreError::Rewrite(e.to_string()))?;
        match cache
            .compare_exchange(&key, expected, Some(Replacement { value, ttl }))
            .await?
        {
            CasOutcome::Applied(_) => return Ok(()),
            CasOutcome::Conflict => continue,
        }
    }
}
