use gproxy_core::{CacheBackend, ControlPlane, CoreError, Settlement, Target};
use gproxy_protocol::SettleMode;

use super::super::AppHost;
use super::auth::unix_now;

pub(super) async fn check(
    host: &AppHost,
    target: &Target,
    settle: SettleMode,
) -> Result<(), CoreError> {
    if settle == SettleMode::Free {
        return Ok(());
    }
    let snapshot = host.services.control.current();
    let Some(quota) = snapshot.quotas.iter().find(|quota| {
        quota.enabled
            && quota.subject_kind == "credential"
            && quota.subject_id == target.credential.0
    }) else {
        return Ok(());
    };
    if quota.limits().next().is_none() {
        return Ok(());
    }
    if host
        .services
        .cache
        .get(&failure_key(quota.id))
        .await?
        .is_some()
    {
        return Err(CoreError::Store(gproxy_core::error::StoreError(
            "credential budget settlement failed; repair accounting before retrying".into(),
        )));
    }
    for (kind, limit) in quota.limits() {
        let window = host
            .services
            .store
            .ensure_quota_window(quota.id, kind, unix_now())
            .await
            .map_err(store_error)?;
        if window.cost_used >= limit {
            return Err(CoreError::QuotaExceeded);
        }
    }
    if host
        .services
        .control
        .pricing(&target.provider, &target.upstream_model)
        .is_none()
    {
        return Err(CoreError::Internal(
            "credential cost limit requires model pricing".into(),
        ));
    }
    Ok(())
}

pub(in crate::host) async fn record(host: &AppHost, settlement: &Settlement) {
    let snapshot = host.services.control.current();
    // A disabled limit still accumulates spend, so re-enabling it cannot reset usage.
    for quota in snapshot.quotas.iter().filter(|quota| {
        quota.subject_kind == "credential" && quota.subject_id == settlement.credential_id.0
    }) {
        let now = unix_now();
        for (kind, _) in quota.limits() {
            let mut persisted = false;
            for _ in 0..3 {
                let result = async {
                    let window = host
                        .services
                        .store
                        .ensure_quota_window(quota.id, kind, now)
                        .await?;
                    host.services
                        .store
                        .add_quota_cost(&settlement.request_id, window.id, settlement.cost)
                        .await
                }
                .await;
                match result {
                    Ok(_) => {
                        persisted = true;
                        break;
                    }
                    Err(error) => {
                        tracing::error!(request_id = %settlement.request_id, quota_id = quota.id, error = %error, "persist credential budget failed")
                    }
                }
            }
            if !persisted
                && let Err(error) = host
                    .services
                    .cache
                    .set(&failure_key(quota.id), vec![1], None)
                    .await
            {
                tracing::error!(quota_id = quota.id, error = %error, "trip credential budget accounting failure failed");
            }
        }
    }
}

fn failure_key(quota_id: i64) -> String {
    format!("gproxy:credential-budget-failed:{quota_id}")
}

fn store_error(error: gproxy_store::StoreError) -> CoreError {
    CoreError::Store(gproxy_core::error::StoreError(error.to_string()))
}
