mod lease;
use crate::AppHandle;
use gproxy_admin::dto::{
    ConnectivityScopeDto, ConnectivityTestRequest, QuotaProbeResponse, QuotaProbeWindowDto,
    QuotaResetCreditsDto,
};
use gproxy_admin::{AdminError, State};
use gproxy_core::{CacheBackend, Host};
use std::time::Duration;

const FRESH: Duration = Duration::from_secs(600);

pub(crate) async fn run(
    app: &AppHandle,
    credential_id: i64,
    force: bool,
) -> Result<QuotaProbeResponse, AdminError> {
    if !app
        .credential_quota_capabilities(credential_id)
        .await?
        .is_some_and(|capability| capability.probe)
    {
        return Err(AdminError::BadRequest(
            "credential has no subscription quota endpoint".into(),
        ));
    }
    let cache = &app.inner.host.services.cache;
    let key = format!("quota:probe:{credential_id}");
    let lease = format!("{key}:lease");
    let result_key = format!("{key}:result");
    let mut owner = vec![0; 16];
    getrandom::fill(&mut owner).map_err(internal)?;
    if !force && let Some(bytes) = cache.get(&key).await.map_err(internal)? {
        let result = serde_json::from_slice(&bytes).map_err(internal)?;
        return enrich(app, credential_id, result).await;
    }
    if !cache
        .compare_and_swap(
            &lease,
            None,
            Some(owner.clone()),
            Some(Duration::from_secs(120)),
        )
        .await
        .map_err(internal)?
    {
        for _ in 0..240 {
            app.inner.host.wait(Duration::from_millis(500)).await;
            if cache.get(&lease).await.map_err(internal)?.is_none() {
                let Some(bytes) = cache.get(&result_key).await.map_err(internal)? else {
                    return Err(AdminError::BadRequest(
                        "quota refresh failed; retry later".into(),
                    ));
                };
                return enrich(
                    app,
                    credential_id,
                    serde_json::from_slice(&bytes).map_err(internal)?,
                )
                .await;
            }
        }
        return Err(AdminError::Conflict(
            "quota refresh is still running".into(),
        ));
    }
    let result = async {
        if !force {
            if let Some(bytes) = cache.get(&key).await.map_err(internal)? {
                return serde_json::from_slice(&bytes).map_err(internal);
            }
            if let Ok(cycles) = app
                .inner
                .host
                .services
                .store
                .credential_quota_cycles(Some(credential_id), 0, crate::quota_refresh::now() + 1)
                .await
                && cycles
                    .iter()
                    .any(|cycle| cycle.last_observed_at > crate::quota_refresh::now() - 600)
            {
                return Ok(QuotaProbeResponse {
                    cycles: Vec::new(),
                    local_error: false,
                    windows: cycles
                        .iter()
                        .filter(|cycle| {
                            cycle.status == gproxy_store::records::QuotaCycleStatus::Open
                        })
                        .map(|cycle| QuotaProbeWindowDto {
                            upstream_used: cycle
                                .upstream_used
                                .map(|value| value.normalize().to_string()),
                            upstream_limit: cycle
                                .upstream_limit
                                .map(|value| value.normalize().to_string()),
                            unit: cycle.tracking.unit.clone(),
                            window_key: cycle.window_key.clone(),
                            label: cycle.label.clone(),
                            used_percent: cycle
                                .used_percent
                                .map(|value| value.normalize().to_string()),
                            period_end: cycle.period_end,
                        })
                        .collect(),
                    reset_credits: None,
                    raw: String::new(),
                });
            }
            if cache
                .get(&format!("quota:retry:{credential_id}"))
                .await
                .map_err(internal)?
                .is_some()
            {
                return Err(AdminError::Conflict(
                    "quota refresh is backing off; retry later".into(),
                ));
            }
        }
        if cache
            .get(&format!("quota:upstream-retry:{credential_id}"))
            .await
            .map_err(internal)?
            .is_some()
        {
            return Err(AdminError::Conflict(
                "upstream requested a longer quota retry interval".into(),
            ));
        }
        let slot = lease::acquire(app, &owner).await?;
        let result = async {
            cache.delete(&result_key).await.map_err(internal)?;
            let fetch = fetch(app, credential_id);
            let timeout = app.inner.host.wait(Duration::from_secs(90));
            futures_util::pin_mut!(fetch, timeout);
            let result = match futures_util::future::select(fetch, timeout).await {
                futures_util::future::Either::Left((result, _)) => result,
                futures_util::future::Either::Right(_) => {
                    Err(AdminError::Conflict("quota refresh timed out".into()))
                }
            };
            match &result {
                Ok(result) => {
                    let bytes = serde_json::to_vec(result).map_err(internal)?;
                    cache
                        .set(&key, bytes.clone(), Some(FRESH))
                        .await
                        .map_err(internal)?;
                    cache
                        .set(&result_key, bytes, Some(FRESH))
                        .await
                        .map_err(internal)?;
                    cache
                        .delete(&format!("quota:failures:{credential_id}"))
                        .await
                        .map_err(internal)?;
                    cache
                        .delete(&format!("quota:retry:{credential_id}"))
                        .await
                        .map_err(internal)?;
                }
                Err(_) => {
                    let failures = cache
                        .incr(
                            &format!("quota:failures:{credential_id}"),
                            1,
                            Some(Duration::from_secs(86400)),
                        )
                        .await
                        .map_err(internal)?;
                    let delay = (600 * (1u64 << (failures - 1).clamp(0, 3))).min(3600);
                    cache
                        .set(
                            &format!("quota:retry:{credential_id}"),
                            vec![1],
                            Some(Duration::from_secs(delay)),
                        )
                        .await
                        .map_err(internal)?;
                }
            }
            result
        }
        .await;
        lease::release(app, slot, &owner).await?;
        result
    }
    .await;
    if let Ok(value) = &result {
        cache
            .set(
                &result_key,
                serde_json::to_vec(value).map_err(internal)?,
                Some(FRESH),
            )
            .await
            .map_err(internal)?;
    }
    cache
        .compare_and_swap(&lease, Some(owner), None, None)
        .await
        .map_err(internal)?;
    enrich(app, credential_id, result?).await
}

async fn fetch(app: &AppHandle, credential_id: i64) -> Result<QuotaProbeResponse, AdminError> {
    let (provider, _) = super::connectivity::target::resolve(
        app,
        &ConnectivityTestRequest {
            scope: ConnectivityScopeDto::Credential,
            provider_id: None,
            credential_id: Some(credential_id),
            proxy_url: None,
        },
    )?;
    let result = app
        .inner
        .core
        .quota_probe(
            &provider.channel.clone(),
            &provider,
            gproxy_core::CredentialId(credential_id),
        )
        .await;
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            if let gproxy_core::CoreError::RateLimited { retry_after_secs } = error {
                app.inner
                    .host
                    .services
                    .cache
                    .set(
                        &format!("quota:upstream-retry:{credential_id}"),
                        vec![1],
                        Some(Duration::from_secs(u64::from(retry_after_secs))),
                    )
                    .await
                    .map_err(internal)?;
            }
            return Err(AdminError::BadRequest(format!(
                "quota probe failed: {error}"
            )));
        }
    };
    Ok(QuotaProbeResponse {
        cycles: Vec::new(),
        local_error: false,
        windows: result
            .observations
            .into_iter()
            .map(|observation| QuotaProbeWindowDto {
                upstream_used: observation
                    .upstream_used
                    .map(|value| value.normalize().to_string()),
                upstream_limit: observation
                    .upstream_limit
                    .map(|value| value.normalize().to_string()),
                unit: observation.unit,
                window_key: observation.window_key,
                label: observation.label,
                used_percent: observation
                    .used_percent
                    .map(|value| value.normalize().to_string()),
                period_end: observation.period_end,
            })
            .collect(),
        reset_credits: result.reset_credits.map(|credits| QuotaResetCreditsDto {
            available_count: credits.available_count,
            expires_at: credits.expires_at,
        }),
        raw: result.raw,
    })
}

async fn enrich(
    app: &AppHandle,
    credential_id: i64,
    mut result: QuotaProbeResponse,
) -> Result<QuotaProbeResponse, AdminError> {
    let now = crate::quota_refresh::now();
    let store = &app.inner.host.services.store;
    if let Err(error) = store.repair_credential_quota(credential_id, now).await {
        tracing::warn!(error = %error, credential_id, "quota accounting repair failed");
        result.local_error = true;
    }
    match store
        .credential_quota_cycles(Some(credential_id), 0, now + 1)
        .await
    {
        Ok(cycles) => result.cycles = cycles.iter().map(Into::into).collect(),
        Err(error) => {
            tracing::warn!(error = %error, credential_id, "quota local statistics unavailable");
            result.local_error = true;
        }
    }
    if !app.inner.host.services.control.settings().enable_usage {
        result.local_error = true;
        for cycle in &mut result.cycles {
            cycle.metrics = serde_json::json!({});
            cycle.models.clear();
            cycle.estimate = None;
        }
    }
    Ok(result)
}

fn internal(error: impl std::fmt::Display) -> AdminError {
    AdminError::Internal(error.to_string())
}
