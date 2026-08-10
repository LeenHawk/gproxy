//! Settle-time reconciliation (M6 §17): refund the authz-time quota pending
//! by the exact pre-deducted amount, persist actual cost into every quota row
//! on the identity's scope chain, and feed the M3 (`rlt:*` daily token) and
//! M4 (`ctpm:*` per-credential tpm) counter seams.

use std::time::Duration;

use rust_decimal::Decimal;

use super::SettleCtx;
use crate::app::AppState;
use crate::billing::pending;
use crate::store::persistence::records::Scope;
use crate::usage::NormalizedUsage;
use crate::util::time::unix_now;

pub(super) async fn reconcile(ctx: &SettleCtx, usage: &NormalizedUsage, cost: Decimal) {
    reconcile_target(
        ReconcileTarget {
            state: &ctx.state,
            request_id: &ctx.request_id,
            credential_id: ctx.credential.id,
            pending_micros: ctx.pending_micros,
            quota_scopes: &ctx.quota_scopes,
            token_rlt_ids: &ctx.token_rlt_ids,
        },
        usage,
        cost,
    )
    .await;
}

pub(super) struct ReconcileTarget<'a> {
    pub(super) state: &'a AppState,
    pub(super) request_id: &'a str,
    pub(super) credential_id: i64,
    pub(super) pending_micros: i64,
    pub(super) quota_scopes: &'a [(Scope, i64)],
    pub(super) token_rlt_ids: &'a [i64],
}

pub(super) async fn reconcile_target(
    target: ReconcileTarget<'_>,
    usage: &NormalizedUsage,
    cost: Decimal,
) {
    let cache = target.state.cache.as_ref();

    // Exact refund of the pre-deduct — same amount, never recomputed. (If a
    // crash loses this, the 15-minute pending TTL self-heals.)
    pending::refund(
        cache,
        target.quota_scopes,
        target.pending_micros,
        target.request_id,
    )
    .await;

    // Persist actual cost on every scope that has a quota row. The increment is
    // atomic per row (`add_quota_cost`): the M6 read-modify-write lost-update
    // race across instances is closed.
    if cost > Decimal::ZERO {
        let db = target.state.persistence.as_ref();
        for &(scope, scope_id) in target.quota_scopes {
            if let Err(e) = db.add_quota_cost(scope, scope_id, cost).await {
                tracing::warn!(
                    request_id = %target.request_id,
                    scope = scope.as_str(),
                    scope_id,
                    operation = "add_quota_cost",
                    error = %e,
                    "quota reconcile write failed"
                );
            }
        }
    }

    // Counter feeds: actual total tokens of this request. Best-effort — a
    // backend failure is logged there; the next precheck fails closed anyway.
    let total = i64::try_from(usage.total()).unwrap_or(i64::MAX);
    if total > 0 {
        let now = unix_now();
        // M3 seam: authz precheck reads the daily `rlt:{row_id}:d{day}` budget.
        for id in target.token_rlt_ids {
            let key = format!("rlt:{id}:d{}", now / 86_400);
            if let Err(error) = cache
                .incr(&key, total, Some(Duration::from_secs(48 * 3600)))
                .await
            {
                tracing::warn!(
                    request_id = %target.request_id,
                    scope = "rate_limit",
                    scope_id = id,
                    operation = "increment_token_counter",
                    error = %error,
                    "settle token counter update failed"
                );
            }
        }
        // M4 seam: failover's per-credential tpm budget reads `ctpm:{id}:m{min}`.
        let key = format!("ctpm:{}:m{}", target.credential_id, now / 60);
        if let Err(error) = cache
            .incr(&key, total, Some(Duration::from_secs(120)))
            .await
        {
            tracing::warn!(
                request_id = %target.request_id,
                scope = "credential",
                scope_id = target.credential_id,
                operation = "increment_token_counter",
                error = %error,
                "settle token counter update failed"
            );
        }
    }
}
