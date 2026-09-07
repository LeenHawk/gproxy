use gproxy_channel_api::{Channel, Disposition};

use crate::control::Target;
use crate::host::Host;

pub(crate) async fn response(
    host: &impl Host,
    channel: &dyn Channel,
    facts: &super::FunnelCtx,
    disposition: Disposition,
    status: http::StatusCode,
    headers: &http::HeaderMap,
) {
    let target = &facts.target;
    let mut observations = channel.observe_quota(headers);
    let received_at_ms = crate::quota::now_ms();
    for observation in &mut observations {
        observation.sample = Some(gproxy_channel_api::QuotaSample {
            source: gproxy_channel_api::QuotaSampleSource::Response,
            started_at_ms: facts
                .upstream_started_at_ms
                .expect("upstream response has a send time"),
            received_at_ms,
        });
    }
    if !observations.is_empty() {
        host.observe_credential_quota(target.credential, observations)
            .await;
    }
    let Some(credential_version) = facts.credential_version else {
        return;
    };
    let (health, detail) = match disposition {
        Disposition::Success => (
            crate::CredentialHealth::Healthy,
            "upstream request succeeded",
        ),
        Disposition::Retryable => (
            crate::CredentialHealth::Degraded,
            "retryable upstream response",
        ),
        Disposition::Terminal => (crate::CredentialHealth::Healthy, "terminal client response"),
        Disposition::CredentialDead => (
            crate::CredentialHealth::Dead,
            "credential rejected upstream",
        ),
    };
    host.record_credential_health(
        target.credential,
        &target.upstream_model,
        credential_version,
        health,
        Some(status),
        detail,
    )
    .await;
}

pub(crate) async fn degraded(
    host: &impl Host,
    target: &Target,
    credential_version: Option<u64>,
    status: Option<http::StatusCode>,
    detail: &str,
) {
    let Some(credential_version) = credential_version else {
        return;
    };
    host.record_credential_health(
        target.credential,
        &target.upstream_model,
        credential_version,
        crate::CredentialHealth::Degraded,
        status,
        detail,
    )
    .await;
}

pub(crate) async fn dead(
    host: &impl Host,
    target: &Target,
    credential_version: Option<u64>,
    detail: &str,
) {
    let Some(credential_version) = credential_version else {
        return;
    };
    host.record_credential_health(
        target.credential,
        &target.upstream_model,
        credential_version,
        crate::CredentialHealth::Dead,
        None,
        detail,
    )
    .await;
}
