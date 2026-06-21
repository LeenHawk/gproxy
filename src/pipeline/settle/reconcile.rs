//! Settle-time reconciliation (M6 §17): refund the authz-time quota pending
//! by the exact pre-deducted amount, persist actual cost into every quota row
//! on the identity's scope chain, and feed the M3 (`rlt:*` daily token) and
//! M4 (`ctpm:*` per-credential tpm) counter seams.

use std::time::Duration;

use rust_decimal::Decimal;

use super::SettleCtx;
use crate::billing::pending;
use crate::usage::NormalizedUsage;
use crate::util::time::unix_now;

pub(super) async fn reconcile(ctx: &SettleCtx, usage: &NormalizedUsage, cost: Decimal) {
    let cache = ctx.state.cache.as_ref();

    // Exact refund of the pre-deduct — same amount, never recomputed. (If a
    // crash loses this, the 15-minute pending TTL self-heals.)
    pending::refund(cache, &ctx.quota_scopes, ctx.pending_micros).await;

    // Persist actual cost on every scope that has a quota row. The increment is
    // atomic per row (`add_quota_cost`): the M6 read-modify-write lost-update
    // race across instances is closed.
    if cost > Decimal::ZERO {
        let db = ctx.state.persistence.as_ref();
        for &(scope, scope_id) in &ctx.quota_scopes {
            if let Err(e) = db.add_quota_cost(scope, scope_id, cost).await {
                tracing::warn!(request_id = %ctx.request_id, error = %e, "quota reconcile write failed");
            }
        }
    }

    // Counter feeds: actual total tokens of this request. Best-effort — a
    // backend failure is logged there; the next precheck fails closed anyway.
    let total = i64::try_from(usage.total()).unwrap_or(i64::MAX);
    if total > 0 {
        let now = unix_now();
        // M3 seam: authz precheck reads the daily `rlt:{row_id}:d{day}` budget.
        for id in &ctx.token_rlt_ids {
            let key = format!("rlt:{id}:d{}", now / 86_400);
            let _ = cache
                .incr(&key, total, Some(Duration::from_secs(48 * 3600)))
                .await;
        }
        // M4 seam: failover's per-credential tpm budget reads `ctpm:{id}:m{min}`.
        let key = format!("ctpm:{}:m{}", ctx.credential.id, now / 60);
        let _ = cache
            .incr(&key, total, Some(Duration::from_secs(120)))
            .await;
    }
}
