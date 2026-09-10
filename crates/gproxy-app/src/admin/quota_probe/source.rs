use super::{internal, now_ms};
use crate::AppHandle;
use gproxy_admin::AdminError;
use gproxy_channel_api::{QuotaRefreshError, QuotaSource, QuotaSourceState};
use gproxy_core::{CacheBackend, CoreError, CredentialId, Host, ProviderRef};
use std::time::Duration;

pub(super) async fn refresh(
    app: &AppHandle,
    provider: &ProviderRef,
    id: i64,
    version: u64,
    capability: QuotaSource,
    force: bool,
    owner: &[u8],
) -> Result<Option<String>, AdminError> {
    let cache = &app.inner.host.services.cache;
    let key = format!("quota:source:{id}:v{version}:{}", capability.id);
    if cache
        .get(&format!("{key}:upstream-retry"))
        .await
        .map_err(internal)?
        .is_some()
        || (!force
            && cache
                .get(&format!("{key}:retry"))
                .await
                .map_err(internal)?
                .is_some())
    {
        return Ok(None);
    }
    let slot = super::lease::acquire(app, owner).await?;
    let attempted_at_ms = now_ms();
    let result = async {
        let fetch =
            app.inner
                .core
                .quota_source(provider, CredentialId(id), version, &capability.id);
        let timeout = app.inner.host.wait(Duration::from_secs(45));
        futures_util::pin_mut!(fetch, timeout);
        match futures_util::future::select(fetch, timeout).await {
            futures_util::future::Either::Left((result, _)) => result,
            futures_util::future::Either::Right(_) => Err(CoreError::UpstreamExhausted(
                "quota refresh timed out".into(),
            )),
        }
    }
    .await;
    super::lease::release(app, slot, owner).await?;
    let mut state = QuotaSourceState {
        capability,
        attempted_at_ms: Some(attempted_at_ms),
        observed_at_ms: None,
        error: None,
        reset_credits: None,
    };
    match result {
        Ok(result) => {
            state.observed_at_ms = Some(result.observed_at_ms);
            state.reset_credits = result.reset_credits;
            app.inner
                .host
                .services
                .store
                .save_credential_quota_source(id, version, &state, Some(&result.entries))
                .await?;
            cache
                .delete(&format!("{key}:failures"))
                .await
                .map_err(internal)?;
            cache
                .delete(&format!("{key}:retry"))
                .await
                .map_err(internal)?;
            Ok(Some(result.raw))
        }
        Err(error) => {
            if let CoreError::RateLimited { retry_after_secs } = &error {
                cache
                    .set(
                        &format!("{key}:upstream-retry"),
                        vec![1],
                        Some(Duration::from_secs(u64::from(*retry_after_secs))),
                    )
                    .await
                    .map_err(internal)?;
            }
            state.error = Some(public_error(&error));
            app.inner
                .host
                .services
                .store
                .save_credential_quota_source(id, version, &state, None)
                .await?;
            let failures = cache
                .incr(
                    &format!("{key}:failures"),
                    1,
                    Some(Duration::from_secs(86400)),
                )
                .await
                .map_err(internal)?;
            let delay = (600 * (1u64 << (failures - 1).clamp(0, 3))).min(3600);
            cache
                .set(
                    &format!("{key}:retry"),
                    vec![1],
                    Some(Duration::from_secs(delay)),
                )
                .await
                .map_err(internal)?;
            Ok(None)
        }
    }
}

fn public_error(error: &CoreError) -> QuotaRefreshError {
    let (code, message) = match error {
        CoreError::RateLimited { .. } => (
            "rate_limited",
            "Upstream requested a longer retry interval.",
        ),
        CoreError::Unsupported => (
            "unsupported",
            "This credential cannot query this quota source.",
        ),
        CoreError::UpstreamExhausted(message) => ("upstream", message.as_str()),
        _ => (
            "unavailable",
            "Quota query failed. Check credential permissions and provider connectivity.",
        ),
    };
    QuotaRefreshError {
        code: code.into(),
        message: message.into(),
    }
}
