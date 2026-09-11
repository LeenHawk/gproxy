//! How an upstream status maps onto a failover decision.
//!
//! The two policies differ on one wire fact: whether the provider reports a
//! dead credential with 402/403, or reserves those for per-request refusals a
//! live credential can still hit. Picking the wrong one either burns a working
//! credential on a content-policy block, or keeps retrying a revoked one.

use gproxy_channel_api::{Disposition, ResponseView};

/// 401 only. The provider answers policy refusals, region blocks, and
/// per-request denials with 403 while the credential itself stays good.
pub(crate) fn unauthorized_only(response: ResponseView<'_>) -> Disposition {
    match response.status.as_u16() {
        200..=299 => Disposition::Success,
        401 => Disposition::CredentialDead,
        429 | 500..=599 => Disposition::Retryable,
        _ => Disposition::Terminal,
    }
}

/// 401–403. The provider reports an exhausted, suspended, or revoked
/// credential with 402 or 403 as readily as with 401.
pub(crate) fn unauthorized_or_forbidden(response: ResponseView<'_>) -> Disposition {
    match response.status.as_u16() {
        200..=299 => Disposition::Success,
        401..=403 => Disposition::CredentialDead,
        429 | 500..=599 => Disposition::Retryable,
        _ => Disposition::Terminal,
    }
}
