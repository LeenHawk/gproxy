//! Budgets and limits, read back: the windows a quota has been through, the
//! settlements inside one of them, what a channel meters a credential by, and
//! what an upstream last said about the account behind it.
//!
//! The write side of the same rows is `manage().quotas()`, and the live status
//! of a budget or a limit is core's — this family does not recompute either.
//! What is here is the history the status calls do not return: closed windows,
//! per-request settlements, observed upstream cycles.

use std::{collections::HashMap, sync::Arc};

use gproxy_core::BudgetOwner;
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::Store;
use gproxy_store::entity::limits::{
    counted_window, credential_block, credential_quota_cycle, quota, quota_settlement, quota_window,
};
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder};

use super::{bounds, filter};
use crate::{
    SdkResult,
    dto::{
        BudgetStatusDto, CountedWindowDto, CredentialBlockDto, CredentialCycleDto,
        CredentialQuotaDto, ListQuery, Page, QuotaObservationDto, QuotaObservationQuery,
        QuotaSettlementDto, QuotaWindowDto, QuotaWindowQuery,
    },
    handle::Inner,
};

pub struct QuotaQueries<'a, C> {
    inner: &'a Arc<Inner<C>>,
}

impl<'a, C> QuotaQueries<'a, C> {
    pub(crate) fn new(inner: &'a Arc<Inner<C>>) -> Self {
        Self { inner }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> QuotaQueries<'_, C> {
    /// One page of windows, newest first, each joined to its quota row.
    ///
    /// The join is done here rather than in SQL: a window keeps a historical
    /// reference to its quota, not a foreign key, so the row may be gone and a
    /// database join would silently drop the window with it. The quota's
    /// fields are therefore `Option`, and `quotaSnapshot` — the quota as it
    /// was when the window opened — is always there.
    pub async fn windows(&self, query: QuotaWindowQuery) -> SdkResult<Page<QuotaWindowDto>> {
        let mut condition = Condition::all();
        if let Some(quota_id) = filter(&query.quota_id) {
            condition = condition.add(quota_window::Column::QuotaId.eq(quota_id));
        }
        // An owner selects quotas first. An owner that owns nothing is an
        // empty page, not every window.
        if query.owner_kind.is_some() || query.owner_id.is_some() {
            let mut owner = Condition::all();
            if let Some(kind) = filter(&query.owner_kind) {
                owner = owner.add(quota::Column::OwnerKind.eq(kind));
            }
            if let Some(id) = filter(&query.owner_id) {
                owner = owner.add(quota::Column::OwnerId.eq(id));
            }
            let ids: Vec<String> = self
                .inner
                .store
                .quotas()
                .query(quota::Entity::find().filter(owner))
                .await?
                .into_iter()
                .map(|row| row.id)
                .collect();
            condition = condition.add(quota_window::Column::QuotaId.is_in(ids));
        }
        let now_ms = crate::rt::now_ms();
        if query.active_only {
            condition = condition.add(
                Condition::all()
                    .add(quota_window::Column::StartsAtMs.lte(now_ms))
                    .add(
                        Condition::any()
                            .add(quota_window::Column::EndsAtMs.is_null())
                            .add(quota_window::Column::EndsAtMs.gt(now_ms)),
                    ),
            );
        }

        let (offset, limit) = bounds(query.page, query.page_size);
        let page = self
            .inner
            .store
            .quota_windows()
            .page(
                quota_window::Entity::find()
                    .filter(condition)
                    .order_by_desc(quota_window::Column::StartsAtMs),
                offset,
                limit,
            )
            .await?;

        let quota_ids: Vec<String> = page
            .items
            .iter()
            .map(|row| row.quota_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let quotas: HashMap<String, quota::Model> = self
            .inner
            .store
            .quotas()
            .get_many(&quota_ids)
            .await?
            .into_iter()
            .flatten()
            .map(|row| (row.id.clone(), row))
            .collect();
        Ok(Page::convert(page, |row| {
            let quota = quotas.get(&row.quota_id);
            QuotaWindowDto {
                active: row.starts_at_ms <= now_ms
                    && row.ends_at_ms.is_none_or(|ends_at| ends_at > now_ms),
                id: row.id,
                quota_id: row.quota_id,
                used: row.used.to_string(),
                starts_at_ms: row.starts_at_ms,
                ends_at_ms: row.ends_at_ms,
                owner_kind: quota.map(|q| q.owner_kind.clone()),
                owner_id: quota.map(|q| q.owner_id.clone()),
                window_key: quota.map(|q| q.window_key.clone()),
                metric: quota.map(|q| q.metric.clone()),
                unit: quota.map(|q| q.unit.clone()),
                limit_value: quota.map(|q| q.limit_value.to_string()),
                period: quota.map(|q| q.period.clone()),
                quota_snapshot: row.quota_snapshot,
            }
        }))
    }

    /// The individual contributions that add up to one window's `used`,
    /// newest first. Only `page`/`page_size` of the query are read; a
    /// settlement has no name to search and no owner of its own.
    pub async fn settlements(
        &self,
        window_id: &str,
        query: ListQuery,
    ) -> SdkResult<Page<QuotaSettlementDto>> {
        let (offset, limit) = query.bounds();
        let page = self
            .inner
            .store
            .quota_settlements()
            .page(
                quota_settlement::Entity::find()
                    .filter(quota_settlement::Column::WindowId.eq(window_id))
                    .order_by_desc(quota_settlement::Column::SettledAtMs),
                offset,
                limit,
            )
            .await?;
        Ok(Page::convert(page, QuotaSettlementDto::from))
    }

    /// What this deployment knows about one credential's upstream quota: its
    /// open cycles, each window's [`CLOSED_CYCLES_PER_WINDOW`] most recent
    /// closed ones, and the blocks still keeping it out of selection.
    ///
    /// Unlike `manage().credentials().quota_read`, the credential row need not
    /// still exist: a cycle is a historical reference, and the point of
    /// reading it here is to look at what a deleted credential did.
    pub async fn credential_cycles(&self, credential_id: &str) -> SdkResult<CredentialQuotaDto> {
        credential_quota(
            &self.inner.store,
            credential_id,
            &self.inner.core.snapshot(),
        )
        .await
    }

    /// One page of a credential's raw quota readings, newest first. See
    /// [`QuotaObservationQuery`] for the default range. Like
    /// [`Self::credential_cycles`], the credential row need not still exist.
    pub async fn credential_observations(
        &self,
        credential_id: &str,
        query: QuotaObservationQuery,
    ) -> SdkResult<Page<QuotaObservationDto>> {
        quota_observations(&self.inner.store, credential_id, query).await
    }

    /// The counted windows covering `now_ms` on one credential, in the meter's
    /// own units.
    ///
    /// These are the raw rows the consumption meter writes, one per declared
    /// dimension: channel-declared ones, plus the synthetic `limit:{quota_id}`
    /// dimension an operator limit is metered through. For that synthetic kind
    /// **`manage().quotas().limit_status` is the authoritative answer**, not
    /// this one — it is core's own view, it knows the quota row behind the
    /// dimension, and it reports `used` as a normalized decimal instead of the
    /// fixed-point atoms a cost meter counts in. This call exists for the
    /// dimensions core has no status for, and for looking at the meter itself.
    pub async fn counted_windows(
        &self,
        credential_id: &str,
        now_ms: i64,
    ) -> SdkResult<Vec<CountedWindowDto>> {
        Ok(self
            .inner
            .store
            .counted_windows()
            .query(
                counted_window::Entity::find()
                    .filter(counted_window::Column::CredentialId.eq(credential_id))
                    .filter(counted_window::Column::WindowStartMs.lte(now_ms))
                    .filter(counted_window::Column::WindowEndMs.gt(now_ms))
                    .order_by_asc(counted_window::Column::Dimension)
                    .order_by_asc(counted_window::Column::WindowStartMs),
            )
            .await?
            .into_iter()
            .map(CountedWindowDto::from)
            .collect())
    }

    /// The current window of every enabled budget of `owners`, opened where it
    /// did not exist yet. Ordered by quota id.
    ///
    /// This is `Core::budget_status` verbatim, which is the authoritative
    /// answer for a caller budget; the same call is on the write side as
    /// `manage().quotas().budget_status`, where it takes the clock instead of
    /// being given one.
    pub async fn budget_status(
        &self,
        owners: &[BudgetOwner],
        now_ms: i64,
    ) -> SdkResult<Vec<BudgetStatusDto>> {
        Ok(self
            .inner
            .core
            .budget_status(owners, now_ms)
            .await?
            .into_iter()
            .map(BudgetStatusDto::from)
            .collect())
    }
}

/// Closed cycles `CredentialQuotaDto` carries per window. Enough to see a
/// trend in a weekly window; a 5-hour window's older history is the
/// observation log's to answer.
pub const CLOSED_CYCLES_PER_WINDOW: u64 = 10;

/// How far back an observation listing reaches by default when the
/// credential has no open cycle to start from.
const DEFAULT_OBSERVATION_SPAN_MS: i64 = 24 * 60 * 60 * 1000;

/// The cycles and live blocks of one credential, whether or not its row
/// still exists. Shared by `query().quota()` and `manage().credentials()`.
pub(crate) async fn credential_quota<C: BatchConnectionTrait>(
    store: &Store<C>,
    credential_id: &str,
    snapshot: &gproxy_core::CoreData,
) -> SdkResult<CredentialQuotaDto> {
    let cycles = store
        .credential_cycles()
        .history(credential_id, CLOSED_CYCLES_PER_WINDOW)
        .await?;
    let now_ms = crate::rt::now_ms();
    let blocks = store
        .credential_blocks()
        .query(
            credential_block::Entity::find()
                .filter(credential_block::Column::CredentialId.eq(credential_id))
                .filter(credential_block::Column::UntilMs.gt(now_ms))
                .order_by_asc(credential_block::Column::UntilMs),
        )
        .await?;
    Ok(CredentialQuotaDto {
        cycles: cycles.into_iter().map(CredentialCycleDto::from).collect(),
        blocks: blocks
            .into_iter()
            .filter(|block| {
                let Some(credential) = snapshot.credentials.get(credential_id) else {
                    return true;
                };
                let Some(provider) = snapshot.providers.get(&credential.provider_id) else {
                    return true;
                };
                serde_json::from_value::<gproxy_core::BlockSource>(block.source.clone())
                    .map_or(true, |source| source.is_enforced(provider, credential))
            })
            .map(CredentialBlockDto::from)
            .collect(),
    })
}

/// One page of one credential's observations in the query's range, newest
/// first, read through the `(credential_id, observed_at_ms)` index.
pub(crate) async fn quota_observations<C: BatchConnectionTrait>(
    store: &Store<C>,
    credential_id: &str,
    query: QuotaObservationQuery,
) -> SdkResult<Page<QuotaObservationDto>> {
    let since_ms = match query.since_ms {
        Some(since_ms) => since_ms,
        None => store
            .credential_cycles()
            .open_of(&[credential_id.to_owned()])
            .await?
            .iter()
            .map(|cycle| cycle.starts_at_ms)
            .min()
            .unwrap_or_else(|| crate::rt::now_ms().saturating_sub(DEFAULT_OBSERVATION_SPAN_MS)),
    };
    let mut select = credential_quota_cycle::Entity::find()
        .filter(credential_quota_cycle::Column::CredentialId.eq(credential_id))
        .filter(credential_quota_cycle::Column::ObservedAtMs.gte(since_ms));
    if let Some(until_ms) = query.until_ms {
        select = select.filter(credential_quota_cycle::Column::ObservedAtMs.lt(until_ms));
    }
    let (offset, limit) = bounds(query.page, query.page_size);
    let page = store
        .credential_quota_cycles()
        .page(
            select.order_by_desc(credential_quota_cycle::Column::ObservedAtMs),
            offset,
            limit,
        )
        .await?;
    Ok(Page::convert(page, QuotaObservationDto::from))
}
