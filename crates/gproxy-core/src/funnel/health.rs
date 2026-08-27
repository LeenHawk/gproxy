use gproxy_channel_api::Disposition;

use crate::control::Target;
use crate::host::Host;

pub(crate) async fn response(
    host: &impl Host,
    target: &Target,
    credential_version: Option<u64>,
    disposition: Disposition,
    status: http::StatusCode,
) {
    let Some(credential_version) = credential_version else {
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
