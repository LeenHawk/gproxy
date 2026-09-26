//! What the caller has spent, what they have left, and what they sent
//! recently.
//!
//! All three are reads through the engine handle, and all three are scoped the
//! same way: **the filter is written, not checked**. [`PortalUsageQuery`] has
//! a `userId` field so a console can post one filter object to both surfaces,
//! and this module overwrites it. There is no branch in which a value supplied
//! by the caller reaches the engine, so there is no branch that can be missed.

use std::collections::BTreeMap;

use gproxy_sdk::dto::{
    LogQuery, UsageGroupBy, UsageGroupQuery, UsageQuery, UsageSummaryDto, UsageTrendQuery,
};
use gproxy_seaorm::BatchConnectionTrait;

use super::Portal;
use crate::{
    AppError, Result,
    admission::budgets,
    dto::{PortalQuotaWindowDto, PortalRequestDto, PortalUsageDto, PortalUsageQuery},
};

/// How many recent requests one call may return. A portal shows a short tail,
/// not a log: anything longer is the operator's view, which has cursor paging
/// and the fields this one drops.
pub const MAX_RECENT_REQUESTS: u64 = 50;

impl<C: BatchConnectionTrait + Send + Sync + 'static> Portal<'_, C> {
    /// The caller's own usage over a range, with an optional grouped cut and
    /// an optional trend.
    ///
    /// The summary is always computed; `groupBy` and `bucketMs` each add one
    /// more scan when they are given and cost nothing when they are not.
    pub async fn usage(&self, query: PortalUsageQuery) -> Result<PortalUsageDto> {
        // Refused rather than ignored: unlike `userId`, dropping these would
        // silently widen the answer to everything the caller spent.
        if query.provider_id.is_some()
            || query.credential_id.is_some()
            || query.group_by == Some(UsageGroupBy::Credential)
        {
            return Err(AppError::invalid(
                "providerId, credentialId and groupBy credential are operator views",
            ));
        }
        let filter = UsageQuery {
            from_ms: query.from_ms,
            to_ms: query.to_ms,
            // The whole of the scoping. Whatever the request carried is gone
            // by the time the engine sees this value.
            user_id: Some(self.caller().user_id.clone()),
            api_key_id: query.api_key_id,
            model: query.model,
            operation: query.operation,
            provider_id: None,
            credential_id: None,
            max_scan_rows: None,
        };
        let usage = self.writer().gproxy().query().usage();

        let summary: UsageSummaryDto = usage.summary(filter.clone()).await?;
        let groups = match query.group_by {
            Some(group_by) => {
                usage
                    .group(UsageGroupQuery {
                        filter: filter.clone(),
                        group_by,
                    })
                    .await?
            }
            None => Vec::new(),
        };
        let trend = match query.bucket_ms {
            Some(bucket_ms) => {
                if filter.from_ms.is_none() || filter.to_ms.is_none() {
                    return Err(AppError::invalid(
                        "a trend needs both fromMs and toMs; buckets are aligned to fromMs",
                    ));
                }
                usage.trend(UsageTrendQuery { filter, bucket_ms }).await?
            }
            None => Vec::new(),
        };

        Ok(PortalUsageDto {
            from_ms: query.from_ms,
            to_ms: query.to_ms,
            summary,
            groups,
            trend,
        })
    }

    /// The current window of every budget that applies to the caller.
    ///
    /// The owners are [`budgets::chain`] of the caller's own binding — the
    /// key, the user, the team, the organization — which is
    /// the same list the data plane charges. So this answers "what will stop
    /// my next request", not an approximation of it, and it cannot name an
    /// owner the caller is not part of: the chain is derived from the caller,
    /// never from a request.
    pub async fn quota(&self) -> Result<Vec<PortalQuotaWindowDto>> {
        let owners = budgets::chain(self.caller());
        let windows = self
            .writer()
            .gproxy()
            .query()
            .quota()
            .budget_status(&owners, crate::now_ms())
            .await?;
        Ok(windows
            .into_iter()
            .map(|status| PortalQuotaWindowDto {
                used_percent: used_percent(&status.used, &status.limit),
                owner_kind: status.owner_kind,
                owner_id: status.owner_id,
                window_key: status.window_key,
                period: status.period,
                model_pattern: status.model_pattern,
                unit: status.unit,
                used: status.used,
                limit: status.limit,
                starts_at_ms: status.starts_at_ms,
                resets_at_ms: status.resets_at_ms,
            })
            .collect())
    }

    /// The caller's own most recent requests, newest first, reduced.
    ///
    /// Gated by `settings.portal_recent_requests_enabled`, which this module
    /// is the only consumer of. Off answers with an **empty list rather than
    /// an error**: the switch is an operator's decision about what the portal
    /// shows, not a statement about this caller, and a 403 would invite them
    /// to go and look for the permission they are missing.
    ///
    /// On what is left out, and why, see the module note on
    /// [`super`]: no bodies, no headers, no URL, no client address, no
    /// credential id, and the provider only as a display name.
    pub async fn recent_requests(&self, limit: u64) -> Result<Vec<PortalRequestDto>> {
        if !self.recent_requests_enabled().await? {
            return Ok(Vec::new());
        }
        let page = self
            .writer()
            .gproxy()
            .query()
            .logs()
            .list(LogQuery {
                user_id: Some(self.caller().user_id.clone()),
                limit: Some(limit.clamp(1, MAX_RECENT_REQUESTS)),
                ..LogQuery::default()
            })
            .await?;

        // Display names come from the live snapshot, so a provider that has
        // since been deleted reports None rather than an id the caller has no
        // use for.
        let names: BTreeMap<String, String> = self
            .writer()
            .gproxy()
            .core()
            .snapshot()
            .providers
            .values()
            .map(|provider| (provider.entity.id.clone(), provider.entity.name.clone()))
            .collect();
        Ok(page
            .items
            .into_iter()
            .map(|row| {
                let provider = row
                    .provider_id
                    .as_ref()
                    .and_then(|id| names.get(id).cloned());
                PortalRequestDto::new(row, provider)
            })
            .collect())
    }

    /// `settings.portal_recent_requests_enabled`.
    ///
    /// Read from the database rather than from [`AppData`](crate::AppData):
    /// the settings row is not part of the identity snapshot, and the one
    /// field of it this crate does keep — the capture switches — is pinned per
    /// request because a request must be captured under the policy it was
    /// admitted under. This is a portal read, so the current value is the
    /// right one. A missing settings row is a broken installation and is
    /// reported as such, exactly as `App::refresh` reports it.
    pub(crate) async fn recent_requests_enabled(&self) -> Result<bool> {
        let settings = self
            .writer()
            .store()
            .settings()
            .get()
            .await?
            .ok_or_else(|| AppError::internal("the global settings row is missing"))?;
        Ok(settings.portal_recent_requests_enabled)
    }
}

/// `used / limit` on the 0..100 scale, to two decimals.
///
/// None when the limit is zero: a zero budget is a subject switched off, and
/// "100% of nothing" is not a bar anybody can draw. Not clamped at the top —
/// settlement can land after a window has been admitted, and a budget that
/// reads 103% is a fact worth showing rather than one worth rounding away.
///
/// Computed in floating point on purpose. Both inputs are exact decimal
/// strings and stay exact in `used` and `limit`; this derived field is for a
/// progress bar, and a ratio is not money.
fn used_percent(used: &str, limit: &str) -> Option<String> {
    let used: f64 = used.parse().ok()?;
    let limit: f64 = limit.parse().ok()?;
    (limit > 0.0).then(|| format!("{:.2}", used / limit * 100.0))
}

#[cfg(test)]
mod tests {
    use super::used_percent;

    #[test]
    fn a_percentage_is_two_decimals_and_not_clamped() {
        assert_eq!(used_percent("0", "100").as_deref(), Some("0.00"));
        assert_eq!(used_percent("2.5", "10").as_deref(), Some("25.00"));
        assert_eq!(used_percent("1", "3").as_deref(), Some("33.33"));
        // An overspend is reported, not rounded away.
        assert_eq!(used_percent("103", "100").as_deref(), Some("103.00"));
    }

    #[test]
    fn a_zero_or_unreadable_limit_has_no_percentage() {
        assert_eq!(used_percent("0", "0"), None);
        assert_eq!(used_percent("5", "0"), None);
        assert_eq!(used_percent("5", "-1"), None);
        assert_eq!(used_percent("five", "10"), None);
        assert_eq!(used_percent("5", "ten"), None);
    }
}
