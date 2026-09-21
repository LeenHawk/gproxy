//! Writing credential availability: blocks (Store row + cache copy) and the
//! cache-only failure streaks that turn into blocks.

use crate::{
    BlockSource, Core, CoreResult, CredentialBlock, CredentialBlocks, FailureStreak, ids, keys,
};
use gproxy_channel::channel::QuotaScope;
use gproxy_protocol::Operation;
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::limits::credential_block;
use sea_orm::ActiveValue::Set;
use std::time::Duration;

/// Consecutive failures on one scope before a cooldown block is written.
pub(crate) const FAILURE_THRESHOLD: u32 = 3;
const FAILURE_COOLDOWN_STEP_MS: i64 = 30_000;
const FAILURE_COOLDOWN_MAX_MS: i64 = 10 * 60_000;
/// Rate limiting without a usable Retry-After.
pub(crate) const DEFAULT_RATE_LIMIT_MS: i64 = 30_000;
const BLOCKS_CACHE_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

pub(crate) fn failure_scope(upstream_model: Option<&str>) -> QuotaScope {
    match upstream_model {
        Some(model) => QuotaScope::Models(vec![model.to_owned()]),
        None => QuotaScope::All,
    }
}

/// `Retry-After` in delta-seconds; HTTP-date forms are not parsed here.
pub(crate) fn retry_after_ms(headers: &http::HeaderMap) -> Option<i64> {
    headers
        .get(http::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<i64>()
        .ok()
        .filter(|s| *s >= 0)
        .map(|s| s.saturating_mul(1000))
}

impl<C> Core<C> {
    async fn write_blocks(
        &self,
        provider_id: &str,
        credential_id: &str,
        blocks: &CredentialBlocks,
    ) -> CoreResult<()> {
        let value =
            serde_json::to_vec(blocks).map_err(|e| crate::CoreError::Rewrite(e.to_string()))?;
        self.cache
            .put(
                &keys::credential_blocks(provider_id, credential_id),
                value,
                BLOCKS_CACHE_TTL,
            )
            .await?;
        Ok(())
    }

    /// A success on this scope clears its streak; a healthy model no longer
    /// carries another model's failures.
    pub(crate) async fn record_success(
        &self,
        provider_id: &str,
        credential_id: &str,
        upstream_model: Option<&str>,
        operation: Operation,
        now_ms: i64,
    ) -> CoreResult<()> {
        let mut blocks = self.read_blocks(provider_id, credential_id).await?;
        let scope = failure_scope(upstream_model);
        let before = blocks.failures.len();
        blocks
            .failures
            .retain(|s| !(s.scope == scope && s.operation == Some(operation)));
        let changed = before != blocks.failures.len() || blocks.last_success_at_ms.is_none();
        blocks.last_success_at_ms = Some(now_ms);
        if changed {
            self.write_blocks(provider_id, credential_id, &blocks)
                .await?;
        }
        Ok(())
    }
}

impl<C: BatchConnectionTrait> Core<C> {
    /// Persist one block, then refresh the cache copy. Store first: the row is
    /// the record, the cache is the hot copy rebuilt from it on reload.
    pub(crate) async fn record_block(
        &self,
        provider_id: &str,
        credential_id: &str,
        block: CredentialBlock,
        now_ms: i64,
    ) -> CoreResult<()> {
        persist_block(
            &self.store,
            &self.cache,
            provider_id,
            credential_id,
            block,
            now_ms,
        )
        .await
    }

    /// Bump the streak for this scope; at the threshold write a cooldown block
    /// growing with the streak. Returns the block when one was written.
    pub(crate) async fn record_failure(
        &self,
        provider_id: &str,
        credential_id: &str,
        upstream_model: Option<&str>,
        operation: Operation,
        now_ms: i64,
    ) -> CoreResult<Option<CredentialBlock>> {
        let mut blocks = self.read_blocks(provider_id, credential_id).await?;
        let scope = failure_scope(upstream_model);
        let consecutive = match blocks
            .failures
            .iter_mut()
            .find(|s| s.scope == scope && s.operation == Some(operation))
        {
            Some(streak) => {
                streak.consecutive += 1;
                streak.last_failure_at_ms = now_ms;
                streak.consecutive
            }
            None => {
                blocks.failures.push(FailureStreak {
                    scope: scope.clone(),
                    operation: Some(operation),
                    consecutive: 1,
                    last_failure_at_ms: now_ms,
                });
                1
            }
        };
        self.write_blocks(provider_id, credential_id, &blocks)
            .await?;
        if consecutive < FAILURE_THRESHOLD {
            return Ok(None);
        }
        let cooldown = (FAILURE_COOLDOWN_STEP_MS
            .saturating_mul(i64::from(consecutive - FAILURE_THRESHOLD + 1)))
        .min(FAILURE_COOLDOWN_MAX_MS);
        let block = CredentialBlock {
            scope,
            operation: Some(operation),
            until_ms: now_ms + cooldown,
            source: BlockSource::Failures { consecutive },
            observed_at_ms: now_ms,
        };
        self.record_block(provider_id, credential_id, block.clone(), now_ms)
            .await?;
        Ok(Some(block))
    }
}

/// Persist one block, then refresh the cache copy. Store first: the row is
/// the record, the cache is the hot copy.
pub(crate) async fn persist_block<C: BatchConnectionTrait>(
    store: &gproxy_store::Store<C>,
    cache: &std::sync::Arc<dyn gproxy_cache::Cache>,
    provider_id: &str,
    credential_id: &str,
    block: CredentialBlock,
    now_ms: i64,
) -> CoreResult<()> {
    store
        .credential_blocks()
        .create_many(vec![credential_block::ActiveModel {
            id: Set(ids::random_id()),
            credential_id: Set(credential_id.to_owned()),
            scope: Set(serde_json::to_value(&block.scope)
                .map_err(|e| crate::CoreError::Rewrite(e.to_string()))?),
            operation: Set(block.operation.map(|op| op.id().to_owned())),
            until_ms: Set(block.until_ms),
            source: Set(serde_json::to_value(&block.source)
                .map_err(|e| crate::CoreError::Rewrite(e.to_string()))?),
            observed_at_ms: Set(block.observed_at_ms),
        }])
        .await?;
    crate::api::lifecycle::warm_blocks(cache, provider_id, credential_id, vec![block], now_ms).await
}
