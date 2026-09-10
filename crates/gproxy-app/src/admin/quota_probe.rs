mod lease;
mod source;
use crate::AppHandle;
use gproxy_admin::{
    AdminError,
    dto::{QuotaProbeResponse, QuotaProbeWindowDto, QuotaResetCreditsDto},
};
use gproxy_channel_api::{QuotaQueryMode, QuotaSnapshot, QuotaSupport, QuotaValue};
use gproxy_core::{CacheBackend, Host};
use std::time::Duration;

pub(crate) async fn run(
    app: &AppHandle,
    credential_id: i64,
    force: bool,
) -> Result<QuotaProbeResponse, AdminError> {
    refresh(app, credential_id, force, !force).await
}

pub(crate) async fn automatic(app: &AppHandle, credential_id: i64) -> Result<(), AdminError> {
    refresh(app, credential_id, false, true).await.map(|_| ())
}

async fn refresh(
    app: &AppHandle,
    id: i64,
    force: bool,
    automatic: bool,
) -> Result<QuotaProbeResponse, AdminError> {
    let (provider, version, sources) = super::quota_snapshot::sources(app, id).await?;
    let cache = &app.inner.host.services.cache;
    let lease_key = format!("quota:probe:{id}:lease");
    let mut owner = vec![0; 16];
    getrandom::fill(&mut owner).map_err(internal)?;
    let mut acquired = false;
    for _ in 0..240 {
        if cache
            .compare_and_swap(
                &lease_key,
                None,
                Some(owner.clone()),
                Some(Duration::from_secs(300)),
            )
            .await
            .map_err(internal)?
        {
            acquired = true;
            break;
        }
        app.inner.host.wait(Duration::from_millis(500)).await;
    }
    if !acquired {
        return Err(AdminError::Conflict(
            "quota refresh is still running".into(),
        ));
    }
    let result = async {
        let saved = app
            .inner
            .host
            .services
            .store
            .credential_quota_snapshot(id)
            .await?;
        let mut raw = String::new();
        for capability in sources.into_iter().filter(|source| {
            source.mode == QuotaQueryMode::Probe
                && source.support == QuotaSupport::Ready
                && (!automatic || source.automatic)
        }) {
            let previous = saved
                .sources
                .iter()
                .find(|state| state.capability.id == capability.id);
            if !force
                && previous
                    .and_then(|state| state.observed_at_ms)
                    .is_some_and(|time| time > now_ms() - 600_000)
            {
                continue;
            }
            if let Some(body) =
                source::refresh(app, &provider, id, version, capability, force, &owner).await?
            {
                raw = body;
            }
        }
        response(app, id, raw).await
    }
    .await;
    cache
        .compare_and_swap(&lease_key, Some(owner), None, None)
        .await
        .map_err(internal)?;
    result
}

async fn response(app: &AppHandle, id: i64, raw: String) -> Result<QuotaProbeResponse, AdminError> {
    let snapshot = super::quota_snapshot::read(app, id).await?;
    let mut response = QuotaProbeResponse {
        windows: windows(&snapshot),
        reset_credits: snapshot
            .sources
            .iter()
            .find_map(|source| source.reset_credits)
            .map(|value| QuotaResetCreditsDto {
                available_count: value.available_count,
                expires_at: value.expires_at,
            }),
        snapshot,
        cycles: Vec::new(),
        local_error: false,
        raw,
    };
    let store = &app.inner.host.services.store;
    let now = crate::quota_refresh::now();
    if let Err(error) = store.repair_credential_quota(id, now).await {
        tracing::warn!(credential_id = id, error = %error, "quota accounting repair failed");
        response.local_error = true;
    }
    match store.credential_quota_cycles(Some(id), 0, now + 1).await {
        Ok(cycles) => response.cycles = cycles.iter().map(Into::into).collect(),
        Err(error) => {
            tracing::warn!(credential_id = id, error = %error, "quota local statistics unavailable");
            response.local_error = true;
        }
    }
    if !app.inner.host.services.control.settings().enable_usage {
        response.local_error = true;
        for cycle in &mut response.cycles {
            cycle.metrics = serde_json::json!({});
            cycle.models.clear();
            cycle.estimate = None;
        }
    }
    Ok(response)
}

fn windows(snapshot: &QuotaSnapshot) -> Vec<QuotaProbeWindowDto> {
    snapshot
        .entries
        .iter()
        .filter_map(|entry| {
            let QuotaValue::Window(value) = &entry.value else {
                return None;
            };
            Some(QuotaProbeWindowDto {
                upstream_used: value.used.map(|value| value.normalize().to_string()),
                upstream_limit: value.limit.map(|value| value.normalize().to_string()),
                unit: value.unit.clone(),
                window_key: entry.id.clone(),
                label: entry.label.clone(),
                used_percent: value
                    .used_percent
                    .map(|value| value.normalize().to_string()),
                period_end: value.period_end,
            })
        })
        .collect()
}

fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system clock after Unix epoch")
        .as_millis() as i64
}

fn internal(error: impl std::fmt::Display) -> AdminError {
    AdminError::Internal(error.to_string())
}
