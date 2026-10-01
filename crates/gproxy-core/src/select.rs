//! Credential selection inside the permitted set. Never widens to the
//! provider pool; an empty eligible set is an error the caller reports.

use crate::{
    AffinityScope, Core, CoreError, CoreResult, CredentialAffinity, CredentialAffinityKey,
    CredentialBlocks, CredentialData, CredentialStatus, CredentialStrategy, RequestContext, keys,
};
use gproxy_protocol::Operation;
use std::{collections::HashSet, sync::Arc, time::Duration};

/// How long a session keeps its credential without traffic.
const AFFINITY_TTL: Duration = Duration::from_secs(24 * 60 * 60);
/// Rotation counters outlive any realistic idle gap; a lost counter only
/// restarts rotation at the first candidate.
const ROTATION_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

pub(crate) struct Selection {
    pub credential: Arc<CredentialData>,
    /// Present when the request named an agent session that is or becomes
    /// bound to this credential.
    pub assignment: Option<crate::session::AssignmentHandle>,
}

impl<C> Core<C> {
    pub(crate) async fn read_blocks(
        &self,
        provider_id: &str,
        credential_id: &str,
    ) -> CoreResult<CredentialBlocks> {
        Ok(self
            .cache
            .get(&keys::credential_blocks(provider_id, credential_id))
            .await?
            .and_then(|entry| serde_json::from_slice(&entry.value).ok())
            .unwrap_or_default())
    }

    /// `read_blocks` for several credentials in one cache read. Each entry is
    /// as fresh as a `read_blocks` issued at the same moment would be.
    pub(crate) async fn read_blocks_many(
        &self,
        credentials: &[Arc<CredentialData>],
    ) -> CoreResult<Vec<CredentialBlocks>> {
        let keys: Vec<String> = credentials
            .iter()
            .map(|c| keys::credential_blocks(&c.provider_id, &c.id))
            .collect();
        Ok(self
            .cache
            .get_many(&keys)
            .await?
            .into_iter()
            .map(|entry| {
                entry
                    .and_then(|entry| serde_json::from_slice(&entry.value).ok())
                    .unwrap_or_default()
            })
            .collect())
    }
}

impl<C: gproxy_seaorm::BatchConnectionTrait> Core<C> {
    /// Pick one credential from `request.target.credentials` minus `excluded`.
    /// Eligibility: enabled, Active, not retired, not blocked for this
    /// model/operation now. Affinity honours an existing eligible pin before
    /// strategy ordering. Unbound selections rotate among the best candidates.
    pub(crate) async fn select_credential(
        &self,
        request: &RequestContext,
        excluded: &HashSet<String>,
        now_ms: i64,
    ) -> CoreResult<Selection> {
        let provider = &request.target.provider;
        let model = request.target.upstream_model.as_deref();
        let operation: Operation = request.operation.operation;
        // The first dead candidate, as it was when seen: the credential state
        // is shared and can revive while this awaits, so looking it up again
        // afterwards could find none.
        let mut dead = None;
        let mut candidates = Vec::with_capacity(request.target.credentials.len());
        for credential in &request.target.credentials {
            if excluded.contains(&credential.id)
                || credential.provider_id != provider.entity.id
                || !credential.enabled
                || credential.state.is_retired()
            {
                continue;
            }
            let version = credential.state.load();
            if version.status == CredentialStatus::Dead {
                dead.get_or_insert_with(|| CoreError::CredentialDead {
                    credential_id: credential.id.clone(),
                    reason: version.status_reason.clone(),
                });
                continue;
            }
            candidates.push(credential.clone());
        }
        // Every candidate's blocks in one read rather than one per credential.
        let blocks = self.read_blocks_many(&candidates).await?;
        let mut eligible: Vec<(Arc<CredentialData>, CredentialBlocks)> = candidates
            .into_iter()
            .zip(blocks)
            .map(|(credential, mut blocks)| {
                blocks.retain_enforced(provider, &credential);
                (credential, blocks)
            })
            .filter(|(_, blocks)| blocks.blocked_by(model, operation, now_ms).is_none())
            .collect();
        if eligible.is_empty() {
            return Err(dead.unwrap_or(CoreError::NoUsableCredential));
        }

        let affinity_key = request
            .session
            .as_ref()
            .filter(|s| s.is_stable())
            .map(|session| CredentialAffinityKey {
                scope: AffinityScope {
                    scope: request.scope.clone(),
                    session_id: session.id.clone(),
                    source: session.source,
                },
                provider_id: provider.entity.id.clone(),
            });
        let pinned = match (&affinity_key, provider.session_affinity) {
            (Some(key), true) => self
                .cache
                .get(&keys::credential_affinity(key))
                .await?
                .and_then(|entry| serde_json::from_slice::<CredentialAffinity>(&entry.value).ok())
                .and_then(|pin| eligible.iter().position(|(c, _)| c.id == pin.credential_id)),
            _ => None,
        };
        let index = match pinned {
            Some(index) => index,
            None => {
                let mut ids: Vec<&str> = eligible.iter().map(|(c, _)| c.id.as_str()).collect();
                if provider.credential_strategy == CredentialStrategy::EarliestReset {
                    let resets = self
                        .credential_reset_times(&eligible, model, operation, now_ms)
                        .await?;
                    if let Some(earliest) = resets.values().min() {
                        ids.retain(|id| resets.get(*id) == Some(earliest));
                    }
                }
                ids.sort_unstable();
                let signature = ids.join(",");
                let counter = match self
                    .cache
                    .increment(
                        &keys::credential_selection(&provider.entity.id, &signature),
                        1,
                        i64::MAX as u64,
                        ROTATION_TTL,
                    )
                    .await?
                {
                    gproxy_cache::IncrementOutcome::Applied(counter) => counter.value,
                    gproxy_cache::IncrementOutcome::Limited { current } => current,
                };
                let position = usize::try_from((counter - 1) % ids.len() as u64).unwrap_or(0);
                let chosen = ids[position];
                eligible
                    .iter()
                    .position(|(c, _)| c.id == chosen)
                    .expect("chosen id is eligible")
            }
        };
        // An agent session overrides the strategy: it stays on its assigned
        // credential while usable, and reserves the strategy's pick otherwise.
        let (index, assignment) = match self.session_pick(request, &eligible, index, now_ms).await?
        {
            Some((index, handle)) => (index, Some(handle)),
            None => (index, None),
        };
        let (credential, _) = eligible.swap_remove(index);
        Ok(Selection {
            credential,
            assignment,
        })
    }

    /// Record that this session should keep using this credential. Called
    /// after a successful attempt only, so a failing pick is never pinned.
    pub(crate) async fn pin_affinity(
        &self,
        request: &RequestContext,
        credential_id: &str,
        now_ms: i64,
    ) -> CoreResult<()> {
        let Some(session) = request.session.as_ref().filter(|s| s.is_stable()) else {
            return Ok(());
        };
        if !request.target.provider.session_affinity {
            return Ok(());
        }
        let key = CredentialAffinityKey {
            scope: AffinityScope {
                scope: request.scope.clone(),
                session_id: session.id.clone(),
                source: session.source,
            },
            provider_id: request.target.provider.entity.id.clone(),
        };
        let cache_key = keys::credential_affinity(&key);
        let existing = self
            .cache
            .get(&cache_key)
            .await?
            .and_then(|entry| serde_json::from_slice::<CredentialAffinity>(&entry.value).ok());
        let value = CredentialAffinity {
            credential_id: credential_id.to_owned(),
            anchored_at_ms: existing
                .as_ref()
                .filter(|pin| pin.credential_id == credential_id)
                .map_or(now_ms, |pin| pin.anchored_at_ms),
            last_success_at_ms: now_ms,
        };
        self.cache
            .put(
                &cache_key,
                serde_json::to_vec(&value).map_err(|e| CoreError::Rewrite(e.to_string()))?,
                AFFINITY_TTL,
            )
            .await?;
        Ok(())
    }
}
