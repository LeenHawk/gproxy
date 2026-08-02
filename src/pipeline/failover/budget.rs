//! Per-credential request budgets used by the failover loop.

use std::time::Duration;

use crate::app::AppState;
use crate::pipeline::context::Candidate;

/// Per-credential minute budgets (§3.3), incr-then-check on the shared cache.
/// Backend failures count as exhausted so configured limits fail closed.
pub(super) async fn exhausted(state: &AppState, candidate: &Candidate) -> bool {
    let bucket = crate::util::time::unix_now() / 60;
    let ttl = Some(Duration::from_secs(120));
    let rpm = async {
        match candidate.credential.rpm_limit {
            Some(limit) => {
                let key = format!("crpm:{}:m{bucket}", candidate.credential.id);
                !matches!(state.cache.incr(&key, 1, ttl).await, Ok(n) if n <= limit)
            }
            None => false,
        }
    };
    let tpm = async {
        match candidate.credential.tpm_limit {
            Some(limit) => {
                let key = format!("ctpm:{}:m{bucket}", candidate.credential.id);
                !matches!(state.cache.incr(&key, 0, ttl).await, Ok(n) if n <= limit)
            }
            None => false,
        }
    };
    let (rpm_exhausted, tpm_exhausted) = futures_util::join!(rpm, tpm);
    rpm_exhausted || tpm_exhausted
}
